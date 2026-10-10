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
    pub time: NaiveDateTime
}

#[derive(serde::Deserialize, Insertable)]
#[diesel(table_name = crate::schema::points)]
pub struct NewPoint {
    pub lat: f32,
    pub lon: f32,
    pub alt: Option<i16>,
    pub time: NaiveDateTime
}
