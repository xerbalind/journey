use serde_json::Value;

pub type Coordinate = (f32, f32);

/// An ORS position: `[lon, lat]` or `[lon, lat, elevation]`.
pub type Position = Vec<f64>;

/// Mean earth radius in metres, used by [`distance_meters`].
const EARTH_RADIUS_METERS: f64 = 6_371_008.8;

/// Haversine distance in metres between two `(lon, lat)` points given in degrees.
pub fn distance_meters(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lon_a, lat_a) = (a.0.to_radians(), a.1.to_radians());
    let (lon_b, lat_b) = (b.0.to_radians(), b.1.to_radians());
    let half_dlat = (lat_b - lat_a) / 2.0;
    let half_dlon = (lon_b - lon_a) / 2.0;
    let h = half_dlat.sin().powi(2) + lat_a.cos() * lat_b.cos() * half_dlon.sin().powi(2);
    2.0 * EARTH_RADIUS_METERS * h.sqrt().min(1.0).asin()
}

/// Response of `/api/journey.geojsons`: raw GPS lines and predicted route lines, one per day.
#[derive(serde::Serialize)]
pub struct Journey {
    pub points: Vec<GeoJSON>,
    pub routes: Vec<GeoJSON>,
}

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeoJsonFeatureCollection<F> {
    #[serde(rename = "type")]
    pub type_: String,
    pub features: Vec<F>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeoJsonFeature<P, G> {
    #[serde(rename = "type")]
    pub type_: String,
    pub properties: P,
    pub geometry: G,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeoJsonPoint {
    #[serde(rename = "type")]
    pub type_: String,
    pub coordinates: Position,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeoJsonLineString {
    #[serde(rename = "type")]
    pub type_: String,
    /// ORS may return 2D or 3D positions (longitude, latitude, elevation).
    pub coordinates: Vec<Position>,
}
