use chrono::NaiveDateTime;
use diesel::prelude::*;

#[derive(Queryable, Selectable, serde::Serialize)]
#[diesel(table_name = crate::schema::points)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Point {
    pub id: i32,
    pub lat: f32,
    pub lon: f32,
    pub alt: Option<i16>,
    pub time: NaiveDateTime,
    pub processed: bool,
}

#[derive(serde::Deserialize, Insertable)]
#[diesel(table_name = crate::schema::points)]
pub struct NewPoint {
    pub lat: f32,
    pub lon: f32,
    pub alt: Option<i16>,
    pub time: NaiveDateTime,
}

#[derive(Queryable, Selectable, serde::Serialize)]
#[diesel(table_name = crate::schema::temperatures)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Temperature {
    pub id: i32,
    pub value: f32,
    pub time: NaiveDateTime,
}

#[derive(serde::Deserialize, Insertable)]
#[diesel(table_name = crate::schema::temperatures)]
pub struct NewTemperature {
    pub value: f32,
    pub time: NaiveDateTime,
}

#[derive(Queryable, Selectable, serde::Serialize)]
#[diesel(table_name = crate::schema::routes)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Route {
    pub id: i32,
    pub point_id: Option<i32>,
    pub lon: f32,
    pub lat: f32,
    pub alt: Option<i16>,
    pub distance: Option<f64>,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::routes)]
pub struct NewRoute {
    pub point_id: Option<i32>,
    pub lon: f32,
    pub lat: f32,
    pub alt: Option<i16>,
    pub distance: Option<f64>,
}
