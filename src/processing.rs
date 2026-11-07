use std::{sync::Arc, time::Duration};

use deadpool_diesel::postgres::Pool;
use diesel::prelude::*;
use tokio::sync::Notify;

use crate::{
    geojson::{Coordinate, Position, distance_meters},
    models::{NewRoute, Point, Route},
    openrouteservice::{OpenRouteService, OrsError},
};

/// Points per round, per snap request, and coordinates per directions request.
const BATCH_SIZE: usize = 50;
/// How long to wait after a transient ORS failure (rate limit, quota, outage).
const RETRY_DELAY: Duration = Duration::from_secs(15 * 60);

enum RoundError {
    /// A transient ORS failure: wait `RETRY_DELAY`, then retry.
    Ors,
    /// A database failure: give up until the next wake-up.
    Db(String),
}

/// Start the background worker that turns unprocessed points into `routes` rows.
/// It runs once right away (backfill) and again every time the returned `Notify` fires.
pub fn spawn(pool: Pool, ors: OpenRouteService) -> Arc<Notify> {
    let notify = Arc::new(Notify::new());
    notify.notify_one();

    let worker = notify.clone();
    tokio::spawn(async move {
        loop {
            worker.notified().await;
            loop {
                match process_round(&pool, &ors).await {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(RoundError::Ors) => tokio::time::sleep(RETRY_DELAY).await,
                    Err(RoundError::Db(message)) => {
                        eprintln!("processing: database error: {message}");
                        break;
                    }
                }
            }
        }
    });

    notify
}

fn db_error(error: impl ToString) -> RoundError {
    RoundError::Db(error.to_string())
}

/// Process up to `BATCH_SIZE` unprocessed points.
/// Returns `Ok(false)` when there was nothing to process.
async fn process_round(pool: &Pool, ors: &OpenRouteService) -> Result<bool, RoundError> {
    let conn = pool.get().await.map_err(db_error)?;
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
        .map_err(db_error)?
        .map_err(db_error)?;
    drop(conn);

    if points.is_empty() {
        return Ok(false);
    }

    let snapped = snap_all(ors, &points).await?;

    let mut builder = RouteBuilder {
        rows: Vec::new(),
        previous: last_route.map(|route| (f64::from(route.lon), f64::from(route.lat))),
    };
    let mut run: Vec<(&Point, (f64, f64))> = Vec::new();

    for (point, snap) in points.iter().zip(snapped) {
        match snap {
            None => {
                builder.route_run(ors, &run).await?;
                run.clear();
                let raw = (f64::from(point.lon), f64::from(point.lat));
                let distance = builder
                    .previous
                    .map(|previous| distance_meters(previous, raw));
                builder.push(raw, point.alt, Some(point.id), distance);
            }
            Some(location) if builder.previous.is_none() => {
                builder.push(location, point.alt, Some(point.id), None);
            }
            Some(location) => run.push((point, location)),
        }
    }
    builder.route_run(ors, &run).await?;

    let rows = builder.rows;
    let ids: Vec<i32> = points.iter().map(|point| point.id).collect();
    let conn = pool.get().await.map_err(db_error)?;
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
    .map_err(db_error)?
    .map_err(db_error)?;

    Ok(true)
}

async fn snap_all(
    ors: &OpenRouteService,
    points: &[Point],
) -> Result<Vec<Option<(f64, f64)>>, RoundError> {
    let coordinates: Vec<Coordinate> = points.iter().map(|point| (point.lon, point.lat)).collect();
    match ors.snap(&coordinates).await {
        Ok(snapped) => Ok(snapped),
        Err(OrsError::Transient(_)) => Err(RoundError::Ors),
        Err(OrsError::Permanent(_)) => {
            let mut snapped = Vec::with_capacity(coordinates.len());
            for coordinate in coordinates {
                match ors.snap(&[coordinate]).await {
                    Ok(single) => snapped.push(single.into_iter().next().flatten()),
                    Err(OrsError::Permanent(_)) => snapped.push(None),
                    Err(OrsError::Transient(_)) => return Err(RoundError::Ors),
                }
            }
            Ok(snapped)
        }
    }
}

struct RouteBuilder {
    rows: Vec<NewRoute>,
    /// Location of the last route row, stored or about to be stored.
    previous: Option<(f64, f64)>,
}

impl RouteBuilder {
    fn push(
        &mut self,
        location: (f64, f64),
        alt: Option<i16>,
        point_id: Option<i32>,
        distance: Option<f64>,
    ) {
        self.rows.push(NewRoute {
            point_id,
            lon: location.0 as f32,
            lat: location.1 as f32,
            alt,
            distance,
        });
        self.previous = Some(location);
    }

    /// Route from `previous` through the snapped targets of `run`, in chunks of at most
    /// `BATCH_SIZE` coordinates that share their boundary coordinate.
    async fn route_run(
        &mut self,
        ors: &OpenRouteService,
        run: &[(&Point, (f64, f64))],
    ) -> Result<(), RoundError> {
        let Some(start) = self.previous else {
            return Ok(());
        };
        if run.is_empty() {
            return Ok(());
        }

        let waypoints: Vec<(f64, f64)> = std::iter::once(start)
            .chain(run.iter().map(|(_, location)| *location))
            .collect();

        for chunk_start in (0..run.len()).step_by(BATCH_SIZE - 1) {
            let chunk_end = (chunk_start + BATCH_SIZE).min(waypoints.len());
            let chunk = &waypoints[chunk_start..chunk_end];
            let targets = &run[chunk_start..chunk_end - 1];

            match ors.directions(chunk).await {
                Ok(legs) => {
                    for (leg, (point, _)) in legs.into_iter().zip(targets) {
                        self.push_leg(leg, point);
                    }
                }
                Err(OrsError::Transient(_)) => return Err(RoundError::Ors),
                Err(OrsError::Permanent(_)) => {
                    for (pair, (point, target)) in chunk.windows(2).zip(targets) {
                        match ors.directions(pair).await {
                            Ok(legs) => match legs.into_iter().next() {
                                Some(leg) => self.push_leg(leg, point),
                                None => self.push_fallback(pair[0], *target, point),
                            },
                            Err(OrsError::Permanent(_)) => {
                                self.push_fallback(pair[0], *target, point)
                            }
                            Err(OrsError::Transient(_)) => return Err(RoundError::Ors),
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// One row at the snapped target for a leg that can't be routed.
    fn push_fallback(&mut self, from: (f64, f64), target: (f64, f64), point: &Point) {
        let distance = distance_meters(from, target);
        self.push(target, point.alt, Some(point.id), Some(distance));
    }

    /// Store a leg without its first position. The last row gets the point's id.
    fn push_leg(&mut self, leg: Vec<Position>, point: &Point) {
        if leg.len() == 1 {
            let position = &leg[0];
            let alt = elevation(position).or(point.alt);
            self.push(lon_lat(position), alt, Some(point.id), Some(0.0));
            return;
        }

        let last = leg.len().saturating_sub(1);
        for (index, position) in leg.iter().enumerate().skip(1) {
            let location = lon_lat(position);
            let alt = elevation(position).or(point.alt);
            let distance = self
                .previous
                .map(|previous| distance_meters(previous, location));
            let point_id = (index == last).then_some(point.id);
            self.push(location, alt, point_id, distance);
        }
    }
}

/// `(lon, lat)` of a position. The ORS client already rejects positions without both.
fn lon_lat(position: &[f64]) -> (f64, f64) {
    (
        position.first().copied().unwrap_or_default(),
        position.get(1).copied().unwrap_or_default(),
    )
}

/// ORS elevation rounded and clamped to `i16`, if the position has one.
fn elevation(position: &[f64]) -> Option<i16> {
    position.get(2).map(|elevation| {
        elevation
            .round()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
    })
}
