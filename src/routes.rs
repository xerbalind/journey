use axum::{Json, extract::State};
use diesel::prelude::*;
use diesel::{RunQueryDsl, SelectableHelper};
use std::collections::{BTreeMap, HashMap};

use crate::models::{NewTemperature, Temperature};
use crate::{
    error::AppError::{self, Internal},
    geojson::{Coordinate, GeoJSON, Journey},
    models::{NewPoint, Point, Route},
    state::AppState,
};

pub async fn create_point(
    State(state): State<AppState>,
    Json(new_point): Json<NewPoint>,
) -> Result<Json<Point>, AppError> {
    let conn = state.pool.get().await.map_err(|_| Internal)?;
    let res = conn
        .interact(|conn| {
            diesel::insert_into(crate::schema::points::table)
                .values(new_point)
                .returning(Point::as_returning())
                .get_result(conn)
        })
        .await
        .map_err(|_| Internal)?
        .map_err(|_| Internal)?;
    if let Some(processing) = &state.processing {
        processing.notify_one();
    }
    Ok(Json(res))
}

pub async fn create_temperature(
    State(pool): State<deadpool_diesel::postgres::Pool>,
    Json(new_temperature): Json<NewTemperature>,
) -> Result<Json<Temperature>, AppError> {
    let conn = pool.get().await.map_err(|_| Internal)?;
    let res = conn
        .interact(|conn| {
            diesel::insert_into(crate::schema::temperatures::table)
                .values(new_temperature)
                .returning(Temperature::as_returning())
                .get_result(conn)
        })
        .await
        .map_err(|_| Internal)?
        .map_err(|_| Internal)?;
    Ok(Json(res))
}

const MIN_POINTS_PER_DAY: usize = 12;

pub async fn get_geojsons(
    State(pool): State<deadpool_diesel::postgres::Pool>,
) -> Result<Json<Journey>, AppError> {
    let conn = pool.get().await.map_err(|_| Internal)?;

    let points: Vec<Point> = conn
        .interact(|conn| {
            crate::schema::points::table
                .order(crate::schema::points::time.asc())
                .select(Point::as_returning())
                .load(conn)
        })
        .await
        .map_err(|_| Internal)?
        .map_err(|_| Internal)?;

    let routes: Vec<Route> = conn
        .interact(|conn| {
            crate::schema::routes::table
                .order(crate::schema::routes::id.asc())
                .select(Route::as_returning())
                .load(conn)
        })
        .await
        .map_err(|_| Internal)?
        .map_err(|_| Internal)?;

    // Group points by day
    let mut days: BTreeMap<String, Vec<Coordinate>> = BTreeMap::new();
    let mut point_days: HashMap<i32, String> = HashMap::new();

    for point in points {
        let day = point.time.format("%Y-%m-%d").to_string();
        days.entry(day.clone())
            .or_default()
            .push((point.lon, point.lat));
        point_days.insert(point.id, day);
    }

    days.retain(|_, coordinates| coordinates.len() >= MIN_POINTS_PER_DAY);

    // Group predicted route rows by the day of the point each leg leads to
    let mut route_days: BTreeMap<String, Vec<Coordinate>> = BTreeMap::new();
    let mut pending: Vec<Coordinate> = Vec::new();

    for route in routes {
        pending.push((route.lon, route.lat));
        if let Some(point_id) = route.point_id {
            if let Some(day) = point_days.get(&point_id) {
                route_days
                    .entry(day.clone())
                    .or_default()
                    .append(&mut pending);
            }
            pending.clear();
        }
    }

    route_days.retain(|day, _| days.contains_key(day));

    Ok(Json(Journey {
        points: day_lines(days, POINT_COLORS),
        routes: day_lines(route_days, ROUTE_COLORS),
    }))
}

/// `(dark, light)` day colours of the raw GPS lines.
const POINT_COLORS: (&str, &str) = ("#ff7f00", "#ffb366");
/// `(dark, light)` day colours of the predicted route lines.
const ROUTE_COLORS: (&str, &str) = ("#1f6feb", "#79b8ff");

/// One line per day, alternating `(dark, light)` colours, each day starting at the
/// previous day's last coordinate.
fn day_lines(days: BTreeMap<String, Vec<Coordinate>>, (dark, light): (&str, &str)) -> Vec<GeoJSON> {
    let total_days = days.len();
    let mut result = Vec::new();
    let mut previous_last_coord = None;

    for (index, (_, mut coordinates)) in days.into_iter().enumerate() {
        // Connect this day to the previous day's last point.
        if let Some(last_coord) = previous_last_coord {
            coordinates.insert(0, last_coord);
        }

        previous_last_coord = coordinates.last().copied();

        let is_dark = (total_days - 1 - index).is_multiple_of(2);
        let color = if is_dark { dark } else { light };

        result.push(GeoJSON {
            type_: crate::geojson::Type::Feature,
            properties: serde_json::json!({
                "color": color
            }),
            geometry: crate::geojson::Geometry {
                type_: crate::geojson::GeometryType::LineString,
                coordinates,
            },
        });
    }

    result
}

pub async fn get_position(
    State(pool): State<deadpool_diesel::postgres::Pool>,
) -> Result<Json<Coordinate>, AppError> {
    let conn = pool.get().await.map_err(|_| Internal)?;
    let point: Point = conn
        .interact(|conn| {
            crate::schema::points::table
                .order(crate::schema::points::time.desc())
                .select(Point::as_returning())
                .first(conn)
        })
        .await
        .map_err(|_| Internal)?
        .map_err(|_| Internal)?;

    Ok(Json((point.lon, point.lat)))
}
