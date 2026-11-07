use std::{env, time::Duration};

use crate::geojson::{
    Coordinate, GeoJsonFeature, GeoJsonFeatureCollection, GeoJsonLineString, GeoJsonPoint, Position,
};
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

const PROFILE: &str = "cycling-regular";
const SNAP_RADIUS_METERS: f64 = 350.0;

/// An ORS request failure.
#[derive(Debug, thiserror::Error)]
pub enum OrsError {
    /// Rate limit, quota, server or network trouble: retrying later may succeed.
    #[error("transient ORS error: {0}")]
    Transient(String),
    /// The request itself can't be served (bad input, unroutable, unparsable body).
    #[error("permanent ORS error: {0}")]
    Permanent(String),
}

#[derive(Clone)]
pub struct OpenRouteService {
    client: Client,
    base_url: String,
    api_key: String,
}

impl OpenRouteService {
    /// Build a client from `ORS_BASE_URL` and `ORS_API_KEY`.
    pub fn from_env() -> Result<Self, String> {
        let base_url = env::var("ORS_BASE_URL")
            .map_err(|_| "ORS_BASE_URL environment variable is missing".to_owned())?;
        let api_key = env::var("ORS_API_KEY")
            .map_err(|_| "ORS_API_KEY environment variable is missing".to_owned())?;
        reqwest::Url::parse(&base_url).map_err(|error| error.to_string())?;

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| error.to_string())?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_owned(),
            api_key,
        })
    }

    /// Snap each coordinate to the nearest routable point within the snap radius.
    /// The result has the same length as the input: entry `i` is the snapped `(lon, lat)`
    /// of `coordinates[i]`, or `None` when nothing was found in range.
    pub async fn snap(
        &self,
        coordinates: &[Coordinate],
    ) -> Result<Vec<Option<(f64, f64)>>, OrsError> {
        let request = SnappingRequest {
            locations: coordinates.to_vec(),
            radius: SNAP_RADIUS_METERS,
        };
        let url = self.endpoint(&format!("v2/snap/{PROFILE}/geojson"));
        let response: SnapResponse = self.post_json(&url, &request, "application/json").await?;

        let mut snapped = vec![None; coordinates.len()];
        for feature in response.features {
            let index = feature.properties.source_id;
            let slot = snapped.get_mut(index).ok_or_else(|| {
                OrsError::Permanent(format!("ORS snap returned unknown source_id {index}"))
            })?;
            *slot = Some(lon_lat(&feature.geometry.coordinates)?);
        }

        Ok(snapped)
    }

    /// Route through the given `(lon, lat)` waypoints.
    /// Returns one geometry per leg (`coordinates.len() - 1` legs). Each leg starts at its
    /// waypoint and ends at the next one, so consecutive legs share their boundary position.
    pub async fn directions(
        &self,
        coordinates: &[(f64, f64)],
    ) -> Result<Vec<Vec<Position>>, OrsError> {
        let request = DirectionsRequest {
            coordinates: coordinates.to_vec(),
            preference: "recommended",
            elevation: true,
            instructions: false,
        };
        let url = self.endpoint(&format!("v2/directions/{PROFILE}/geojson"));
        let response: DirectionsResponse = self
            .post_json(&url, &request, "application/geo+json")
            .await?;

        let route = response
            .features
            .into_iter()
            .next()
            .ok_or_else(|| OrsError::Permanent("ORS returned no route".to_owned()))?;

        let way_points = route.properties.way_points;
        let positions = route.geometry.coordinates;
        for position in &positions {
            lon_lat(position)?;
        }
        if way_points.len() != coordinates.len() {
            return Err(OrsError::Permanent(format!(
                "ORS returned {} way points for {} coordinates",
                way_points.len(),
                coordinates.len()
            )));
        }

        way_points
            .windows(2)
            .map(|pair| {
                let (start, end) = (pair[0], pair[1]);
                if start > end {
                    return Err(OrsError::Permanent(format!(
                        "ORS returned decreasing way points {start} > {end}"
                    )));
                }
                positions
                    .get(start..=end)
                    .map(<[Position]>::to_vec)
                    .ok_or_else(|| {
                        OrsError::Permanent(format!("ORS way point {end} is out of range"))
                    })
            })
            .collect()
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{path}", self.base_url)
    }

    async fn post_json<Request, Response>(
        &self,
        url: &str,
        request: &Request,
        accept: &'static str,
    ) -> Result<Response, OrsError>
    where
        Request: Serialize + ?Sized,
        Response: DeserializeOwned,
    {
        let response = self
            .client
            .post(url)
            .header(reqwest::header::ACCEPT, accept)
            .header(reqwest::header::AUTHORIZATION, &self.api_key)
            .json(request)
            .send()
            .await
            .map_err(|error| OrsError::Transient(error.to_string()))?;

        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|error| OrsError::Transient(error.to_string()))?;

        if !status.is_success() {
            let body = match serde_json::from_slice::<Value>(&body) {
                Ok(json) => json.to_string(),
                Err(_) => String::from_utf8_lossy(&body).into_owned(),
            };
            let message = format!("ORS returned HTTP {status}: {body}");
            let transient = status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || status == reqwest::StatusCode::FORBIDDEN
                || status.is_server_error();
            return Err(if transient {
                OrsError::Transient(message)
            } else {
                OrsError::Permanent(message)
            });
        }

        serde_json::from_slice(&body).map_err(|error| OrsError::Permanent(error.to_string()))
    }
}

/// The `(lon, lat)` of an ORS position, or `Permanent` when it has fewer than two values.
fn lon_lat(position: &[f64]) -> Result<(f64, f64), OrsError> {
    match position {
        [lon, lat, ..] => Ok((*lon, *lat)),
        _ => Err(OrsError::Permanent(
            "ORS returned a position without lon/lat".to_owned(),
        )),
    }
}

#[derive(Serialize)]
struct SnappingRequest {
    locations: Vec<Coordinate>,
    radius: f64,
}

#[derive(Serialize)]
struct DirectionsRequest {
    coordinates: Vec<(f64, f64)>,
    // ORS options: fastest, shortest, recommended, or custom.
    preference: &'static str,
    elevation: bool,
    instructions: bool,
}

type SnapResponse = GeoJsonFeatureCollection<GeoJsonFeature<SnapProperties, GeoJsonPoint>>;
type DirectionsResponse =
    GeoJsonFeatureCollection<GeoJsonFeature<DirectionProperties, GeoJsonLineString>>;

#[derive(Debug, Clone, Deserialize)]
struct SnapProperties {
    source_id: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct DirectionProperties {
    way_points: Vec<usize>,
}
