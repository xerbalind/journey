use std::time::Duration;

use deadpool_diesel::postgres::Pool;
use diesel::prelude::*;

use crate::{
    geojson::Coordinate,
    models::{NewRoute, Point, Route},
    openrouteservice::{DIRECTIONS_MAX_POINTS, DIRECTIONS_MIN_POINTS, OpenRouteService, OrsError},
};

/// Time between runs
const RUN_INTERVAL: Duration = Duration::from_secs(15 * 60);
/// Time between halved retries after a permanent ORS error
const RUN_INTERVAL_SOFT: Duration = Duration::from_secs(60);
const BATCH_SIZE: usize = DIRECTIONS_MAX_POINTS - 1;

#[derive(Debug, thiserror::Error)]
enum RunError {
    #[error("ORS error: {0}")]
    Ors(String),
    #[error("database error: {0}")]
    Db(String),
}

fn db(error: impl ToString) -> RunError {
    RunError::Db(error.to_string())
}

/// Start the background worker that converts the gps points in routes.
/// Runs run every RUN_INTERVAL and prints any error
pub fn spawn(pool: Pool, ors: OpenRouteService) {
    tokio::spawn(async move {
        loop {
            if let Err(error) = run(&pool, &ors).await {
                eprintln!("{error}");
            }
            tokio::time::sleep(RUN_INTERVAL).await;
        }
    });
}

/// Get the next points that need to be processed
async fn run(pool: &Pool, ors: &OpenRouteService) -> Result<(), RunError> {
    let conn = pool.get().await.map_err(db)?;
    let (points, last_route) = conn
        .interact(|conn| {
            let points = crate::schema::points::table
                .filter(crate::schema::points::processed.eq(false))
                .order((
                    crate::schema::points::time.asc(),
                    crate::schema::points::id.asc(),
                ))
                .limit(BATCH_SIZE as i64)
                .select(Point::as_returning())
                .load(conn)?;
            let last_route = crate::schema::routes::table
                .order(crate::schema::routes::id.desc())
                .select(Route::as_returning())
                .first(conn)
                .optional()?;
            Ok::<_, diesel::result::Error>((points, last_route))
        })
        .await
        .map_err(db)?
        .map_err(db)?;
    drop(conn);

    if points.len() < DIRECTIONS_MIN_POINTS {
        return Ok(());
    }

    let previous = last_route.map(|route| Coordinate(route.lon, route.lat));
    let mut batch: &[Point] = &points;

    loop {
        match process_batch(ors, batch, previous).await {
            Ok(rows) => {
                store(pool, rows, batch).await?;
                return Ok(());
            }
            Err(OrsError::Transient(message)) => return Err(RunError::Ors(message)),
            // If we get a permanent error then that means at least one of the points if unable to
            // be processed
            Err(OrsError::Permanent(_)) => {
                // We already have the minimum amount of points
                // Just insert them as the raw data points in the routes table
                if batch.len() <= DIRECTIONS_MIN_POINTS {
                    let mut builder = RouteBuilder {
                        rows: Vec::new(),
                        previous,
                    };
                    for point in batch {
                        let raw = Coordinate(point.lon, point.lat);
                        let distance = builder.previous.map(|previous| previous.distance(raw));
                        builder.push(raw, point.alt, Some(point.id), distance);
                    }

                    store(pool, builder.rows, batch).await?;
                    return Ok(());
                }
                // Keep dividing the points in 2 until it either works or we get to DIRECTIONS_MIN_POINTS
                tokio::time::sleep(RUN_INTERVAL_SOFT).await;
                batch = &batch[..batch.len().div_ceil(2)];
            }
        }
    }
}

/// Snap points and route them with directions starting from the previous point
async fn process_batch(
    ors: &OpenRouteService,
    points: &[Point],
    previous: Option<Coordinate>,
) -> Result<Vec<NewRoute>, OrsError> {
    let coordinates: Vec<Coordinate> = points
        .iter()
        .map(|point| Coordinate(point.lon, point.lat))
        .collect();
    let snapped = ors.snap(&coordinates).await?;

    let mut builder = RouteBuilder {
        rows: Vec::new(),
        previous,
    };
    let mut pending: Vec<(&Point, Coordinate)> = Vec::new();

    for (point, snap) in points.iter().zip(snapped) {
        match snap {
            // Point couldn't be snapped
            // So process the pending points to build a route
            // And add the new raw point afterwards
            None => {
                builder.process(ors, &pending).await?;
                pending.clear();
                let raw = Coordinate(point.lon, point.lat);
                let distance = builder.previous.map(|previous| previous.distance(raw));
                builder.push(raw, point.alt, Some(point.id), distance);
            }
            // First point ever so just push to the table
            Some(location) if builder.previous.is_none() => {
                builder.push(location, point.alt, Some(point.id), None);
            }
            // Add in the pending queue to build the route
            Some(location) => pending.push((point, location)),
        }
    }

    builder.process(ors, &pending).await?;
    Ok(builder.rows)
}

/// Insert rows into routes and mark points processed
async fn store(pool: &Pool, rows: Vec<NewRoute>, points: &[Point]) -> Result<(), RunError> {
    let ids: Vec<i32> = points.iter().map(|point| point.id).collect();
    let conn = pool.get().await.map_err(db)?;
    conn.interact(move |conn| {
        conn.transaction(|conn| {
            diesel::insert_into(crate::schema::routes::table)
                .values(&rows)
                .execute(conn)?;
            diesel::update(crate::schema::points::table)
                .filter(crate::schema::points::id.eq_any(&ids))
                .set(crate::schema::points::processed.eq(true))
                .execute(conn)?;
            Ok::<_, diesel::result::Error>(())
        })
    })
    .await
    .map_err(db)?
    .map_err(db)?;

    Ok(())
}

struct RouteBuilder {
    rows: Vec<NewRoute>,
    previous: Option<Coordinate>,
}

impl RouteBuilder {
    fn push(
        &mut self,
        location: Coordinate,
        alt: Option<i16>,
        point_id: Option<i32>,
        distance: Option<f64>,
    ) {
        self.rows.push(NewRoute {
            point_id,
            lon: location.0,
            lat: location.1,
            alt,
            distance,
        });
        self.previous = Some(location);
    }

    async fn process(
        &mut self,
        ors: &OpenRouteService,
        run: &[(&Point, Coordinate)],
    ) -> Result<(), OrsError> {
        let Some(start) = self.previous else {
            return Ok(());
        };
        if run.is_empty() {
            return Ok(());
        }

        let waypoints: Vec<Coordinate> = std::iter::once(start)
            .chain(run.iter().map(|(_, location)| *location))
            .collect();

        let legs = ors.directions(&waypoints).await?;
        for (leg, (point, _)) in legs.into_iter().zip(run) {
            self.push_leg(leg, point);
        }

        Ok(())
    }

    fn push_leg(&mut self, leg: Vec<(Coordinate, Option<i16>)>, point: &Point) {
        if let [(location, elevation)] = leg.as_slice() {
            self.push(
                *location,
                elevation.or(point.alt),
                Some(point.id),
                Some(0.0),
            );
            return;
        }

        let last = leg.len().saturating_sub(1);
        for (index, (location, elevation)) in leg.into_iter().enumerate().skip(1) {
            let distance = self.previous.map(|previous| previous.distance(location));
            let point_id = (index == last).then_some(point.id);
            self.push(location, elevation.or(point.alt), point_id, distance);
        }
    }
}
