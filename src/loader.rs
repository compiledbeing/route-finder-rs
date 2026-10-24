//! Loading a road network from JSON (see `data/java_island.json` for the format).

use std::fs;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use crate::graph::{haversine_km, Graph, GraphError, Node};

#[derive(Debug, Error)]
pub enum LoadError {
    // The underlying error is exposed through `source()`, so it is not repeated in the
    // message; `anyhow` (and `{:#}`) print the whole chain.
    #[error("cannot read data file")]
    Read(#[from] std::io::Error),
    #[error("invalid JSON")]
    Parse(#[from] serde_json::Error),
    #[error("road refers to unknown location '{0}'")]
    UnknownLocation(String),
    #[error("road_factor must be >= 1 (a road cannot beat the straight line), got {0}")]
    InvalidRoadFactor(f64),
    #[error(transparent)]
    Graph(#[from] GraphError),
}

#[derive(Deserialize)]
struct Dataset {
    #[serde(default = "default_name")]
    name: String,
    #[serde(default = "default_speed_kmh")]
    default_speed_kmh: f64,
    #[serde(default = "default_road_factor")]
    road_factor: f64,
    nodes: Vec<NodeSpec>,
    roads: Vec<RoadSpec>,
}

#[derive(Deserialize)]
struct NodeSpec {
    id: String,
    name: Option<String>,
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
struct RoadSpec {
    from: String,
    to: String,
    road: Option<String>,
    speed_kmh: Option<f64>,
    distance_km: Option<f64>,
    #[serde(default)]
    one_way: bool,
}

fn default_name() -> String {
    "Untitled network".to_string()
}

fn default_speed_kmh() -> f64 {
    50.0
}

fn default_road_factor() -> f64 {
    1.2
}

/// Reads and validates a road network from a JSON file.
pub fn load_graph(path: impl AsRef<Path>) -> Result<Graph, LoadError> {
    parse_graph(&fs::read_to_string(path)?)
}

/// Parses and validates a road network from JSON text.
pub fn parse_graph(json: &str) -> Result<Graph, LoadError> {
    let dataset: Dataset = serde_json::from_str(json)?;
    if dataset.road_factor.is_nan() || dataset.road_factor < 1.0 {
        return Err(LoadError::InvalidRoadFactor(dataset.road_factor));
    }

    let mut graph = Graph::new();
    graph.set_name(dataset.name);

    for spec in dataset.nodes {
        let name = spec.name.unwrap_or_else(|| spec.id.clone());
        graph.add_node(Node {
            id: spec.id,
            name,
            lat: spec.lat,
            lon: spec.lon,
        })?;
    }

    for spec in dataset.roads {
        let from = graph
            .find(&spec.from)
            .ok_or_else(|| LoadError::UnknownLocation(spec.from.clone()))?;
        let to = graph
            .find(&spec.to)
            .ok_or_else(|| LoadError::UnknownLocation(spec.to.clone()))?;
        // No explicit length? Estimate it: straight line x a detour factor.
        let distance_km = spec.distance_km.unwrap_or_else(|| {
            haversine_km(graph.node(from), graph.node(to)) * dataset.road_factor
        });
        graph.add_road(
            from,
            to,
            distance_km,
            spec.speed_kmh.unwrap_or(dataset.default_speed_kmh),
            spec.road.unwrap_or_else(|| "Unnamed road".to_string()),
            !spec.one_way,
        )?;
    }
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"{
        "nodes": [
            {"id": "a", "lat": 0, "lon": 0},
            {"id": "b", "name": "Bee", "lat": 0, "lon": 1}
        ],
        "roads": [{"from": "a", "to": "Bee"}]
    }"#;

    #[test]
    fn applies_defaults_and_estimates_distance() {
        let g = parse_graph(MINIMAL).unwrap();
        assert_eq!(g.name(), "Untitled network");
        assert_eq!(g.node(0).name, "a"); // name falls back to the id
        let road = &g.roads()[0];
        assert_eq!(road.speed_kmh, 50.0);
        assert_eq!(road.name, "Unnamed road");
        assert!((road.distance_km - 111.195 * 1.2).abs() < 0.05);
        assert!(road.two_way);
    }

    #[test]
    fn reports_unknown_locations_by_name() {
        let json = r#"{"nodes":[{"id":"a","lat":0,"lon":0}],
                       "roads":[{"from":"a","to":"nowhere"}]}"#;
        let err = parse_graph(json).unwrap_err();
        assert!(matches!(&err, LoadError::UnknownLocation(id) if id == "nowhere"));
        assert!(err.to_string().contains("nowhere"));
    }

    #[test]
    fn surfaces_graph_validation_errors() {
        let json = r#"{"nodes":[{"id":"a","lat":0,"lon":0},{"id":"b","lat":0,"lon":1}],
                       "roads":[{"from":"a","to":"b","distance_km":10}]}"#;
        assert!(matches!(
            parse_graph(json),
            Err(LoadError::Graph(GraphError::ShorterThanStraightLine { .. }))
        ));
    }

    #[test]
    fn rejects_malformed_json_and_bad_road_factor() {
        assert!(matches!(parse_graph("{ nope"), Err(LoadError::Parse(_))));
        let json = r#"{"road_factor":0.5,"nodes":[],"roads":[]}"#;
        assert!(matches!(
            parse_graph(json),
            Err(LoadError::InvalidRoadFactor(_))
        ));
    }

    #[test]
    fn missing_file_is_a_read_error() {
        assert!(matches!(
            load_graph("this/file/does/not/exist.json"),
            Err(LoadError::Read(_))
        ));
    }
}
