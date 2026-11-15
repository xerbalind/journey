use serde_json::Value;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Coordinate(pub f32, pub f32);

const EARTH_RADIUS_METERS: f64 = 6_371_008.8;

impl Coordinate {
    pub fn distance(self, other: Coordinate) -> f64 {
        let (lon_a, lat_a) = (
            f64::from(self.0).to_radians(),
            f64::from(self.1).to_radians(),
        );
        let (lon_b, lat_b) = (
            f64::from(other.0).to_radians(),
            f64::from(other.1).to_radians(),
        );
        let half_dlat = (lat_b - lat_a) / 2.0;
        let half_dlon = (lon_b - lon_a) / 2.0;
        let h = half_dlat.sin().powi(2) + lat_a.cos() * lat_b.cos() * half_dlon.sin().powi(2);
        2.0 * EARTH_RADIUS_METERS * h.sqrt().min(1.0).asin()
    }
}

/// Response of `/api/journey.geojsons`
/// Points == Raw GPS points
/// Routes == Predicted route
#[derive(serde::Serialize)]
pub struct Journey {
    pub points: Vec<GeoJSON>,
    pub routes: Vec<GeoJSON>,
}

#[derive(serde::Serialize)]
#[serde(tag = "type", rename = "Feature")]
pub struct GeoJSON {
    pub properties: Value,
    pub geometry: Geometry,
}

#[derive(serde::Serialize)]
#[serde(tag = "type", rename = "LineString")]
pub struct Geometry {
    pub coordinates: Vec<Coordinate>,
}
