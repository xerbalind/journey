use axum::{Json, extract::State};
use diesel::prelude::*;
use diesel::{RunQueryDsl, SelectableHelper};
use std::collections::BTreeMap;

use crate::{
    error::AppError::{self, Internal},
    geojson::{Coordinate, GeoJSON},
    models::{NewPoint, Point},
};

pub async fn create_point(
    State(pool): State<deadpool_diesel::postgres::Pool>,
    Json(new_point): Json<NewPoint>,
) -> Result<Json<Point>, AppError> {
    let conn = pool.get().await.map_err(|_| Internal)?;
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
    Ok(Json(res))
}

pub async fn get_geojsons(
    State(pool): State<deadpool_diesel::postgres::Pool>,
) -> Result<Json<Vec<GeoJSON>>, AppError> {
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

    // Group points by day
    let mut days: BTreeMap<String, Vec<(f32, f32)>> = BTreeMap::new();

    for point in points {
        let day = point.time.format("%Y-%m-%d").to_string();
        days.entry(day).or_default().push((point.lon, point.lat));
    }

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
        let color = if is_dark { "#ff7f00" } else { "#ffb366" };

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

    Ok(Json(result))
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
