use std::{env, time::Duration};

use crate::geojson::Coordinate;
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

// General
const PROFILE: &str = "cycling-regular";

// Snap
const SNAP_RADIUS_METERS: f64 = 350.0;
pub const SNAP_MAX_POINTS: usize = 5000;

// Directions
const DIRECTIONS_MAX_DISTANCE_M: f64 = 5000000.0;
pub const DIRECTIONS_MAX_POINTS: usize = 50;
pub const DIRECTIONS_MIN_POINTS: usize = 2;
const DIRECTIONS_PREFERENCE: &str = "recommended";

#[derive(Debug, thiserror::Error)]
pub enum OrsError {
    /// Rate limit, quota, server, network trouble, ...
    /// Retrying later may succeed
    #[error("transient ORS error: {0}")]
    Transient(String),
    /// Permanent error
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
    /// The result has the same length as the input
    /// Coordinate can be none when nothing was found
    pub async fn snap(
        &self,
        coordinates: &[Coordinate],
    ) -> Result<Vec<Option<Coordinate>>, OrsError> {
        if coordinates.len() > SNAP_MAX_POINTS {
            return Err(OrsError::Permanent(format!(
                "Too many snap points at once. {} given and {} is max.",
                coordinates.len(),
                SNAP_MAX_POINTS,
            )));
        }

        let request = SnappingRequest {
            locations: coordinates.to_vec(),
            radius: SNAP_RADIUS_METERS,
        };
        let url = self.endpoint(&format!("v2/snap/{PROFILE}/json"));
        let response: SnapResponse = self.post_json(&url, &request, "application/json").await?;

        if response.locations.len() != coordinates.len() {
            return Err(OrsError::Permanent(format!(
                "ORS snap returned {} locations for {} coordinates",
                response.locations.len(),
                coordinates.len()
            )));
        }

        Ok(response
            .locations
            .into_iter()
            .map(|snapped| snapped.map(|snapped| snapped.location))
            .collect())
    }

    /// Route through the given points waypoints.
    /// Returns one geometry per leg (`coordinates.len() - 1` legs) as `(coordinate, elevation)`
    /// pairs, elevation in whole metres when ORS has one. Each leg starts at its waypoint and
    /// ends at the next one, so consecutive legs share their boundary position.
    pub async fn directions(
        &self,
        coordinates: &[Coordinate],
    ) -> Result<Vec<Vec<(Coordinate, Option<i16>)>>, OrsError> {
        if coordinates.len() > DIRECTIONS_MAX_POINTS {
            return Err(OrsError::Permanent(format!(
                "Too many direction points at once. {} given and {} is max.",
                coordinates.len(),
                DIRECTIONS_MAX_POINTS,
            )));
        }
        if coordinates.len() < DIRECTIONS_MIN_POINTS {
            return Err(OrsError::Permanent(format!(
                "At least {DIRECTIONS_MIN_POINTS} direction points are required."
            )));
        }
        if coordinates[0].distance(coordinates[coordinates.len() - 1]) > DIRECTIONS_MAX_DISTANCE_M {
            return Err(OrsError::Permanent(format!(
                "{} is too big of a distance between the first and last point. Max is {}.",
                coordinates[0].distance(coordinates[coordinates.len() - 1]),
                DIRECTIONS_MAX_DISTANCE_M,
            )));
        }

        let request = DirectionsRequest {
            coordinates: coordinates.to_vec(),
            preference: DIRECTIONS_PREFERENCE,
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
        let positions = route
            .geometry
            .coordinates
            .iter()
            .map(|position| Ok((lon_lat(position)?, elevation(position))))
            .collect::<Result<Vec<_>, OrsError>>()?;
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
                    .map(<[_]>::to_vec)
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

/// An ORS position: `[lon, lat]` or `[lon, lat, elevation]`.
type ORSPosition = Vec<f64>;

fn lon_lat(position: &[f64]) -> Result<Coordinate, OrsError> {
    match position {
        [lon, lat, ..] => Ok(Coordinate(*lon as f32, *lat as f32)),
        _ => Err(OrsError::Permanent(
            "ORS returned a position without lon/lat".to_owned(),
        )),
    }
}

fn elevation(position: &[f64]) -> Option<i16> {
    position.get(2).map(|elevation| {
        elevation
            .round()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
    })
}

// Snap types

#[derive(Serialize)]
struct SnappingRequest {
    locations: Vec<Coordinate>,
    radius: f64,
}

#[derive(Deserialize)]
struct SnapResponse {
    locations: Vec<Option<SnapLocation>>,
}

#[derive(Deserialize)]
struct SnapLocation {
    location: Coordinate,
}

// Direction types

#[derive(Serialize)]
struct DirectionsRequest {
    coordinates: Vec<Coordinate>,
    preference: &'static str, // fastest, shortest or recommended
    elevation: bool,
    instructions: bool,
}

#[derive(Deserialize)]
struct DirectionsResponse {
    features: Vec<RouteFeature>,
}

#[derive(Deserialize)]
struct RouteFeature {
    properties: DirectionProperties,
    geometry: LineString,
}

#[derive(Deserialize)]
struct DirectionProperties {
    way_points: Vec<usize>,
}

#[derive(Deserialize)]
struct LineString {
    coordinates: Vec<ORSPosition>,
}
