use axum::{Json, extract::State};
use diesel::prelude::*;
use diesel::{RunQueryDsl, SelectableHelper};

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

pub async fn get_geojson(
    State(pool): State<deadpool_diesel::postgres::Pool>,
) -> Result<Json<GeoJSON>, AppError> {
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

    let coordinates = points.into_iter().map(|p| (p.lon, p.lat)).collect();

    let geojson = GeoJSON {
        type_: crate::geojson::Type::Feature,
        properties: serde_json::json!({}),
        geometry: crate::geojson::Geometry {
            type_: crate::geojson::GeometryType::LineString,
            coordinates,
        },
    };

    Ok(Json(geojson))
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
