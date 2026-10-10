use serde_json::Value;

pub type Coordinate = (f32, f32);

#[derive(serde::Serialize)]
pub enum GeometryType {
    LineString,
}

#[derive(serde::Serialize)]
pub enum Type {
    Feature,
}

#[derive(serde::Serialize)]
pub struct Geometry {
    #[serde(rename = "type")]
    pub type_: GeometryType,
    pub coordinates: Vec<Coordinate>,
}

#[derive(serde::Serialize)]
pub struct GeoJSON {
    #[serde(rename = "type")]
    pub type_: Type,
    pub properties: Value,
    pub geometry: Geometry,
}
