//! JSON shapes shared by the CLI (`--json`) and the HTTP API.
//!
//! Locations are referred to by their string ids, so the output does not depend on
//! internal indices and the web UI can use it as-is.

use serde::Serialize;

use crate::graph::Graph;
use crate::pathfinder::RouteResult;

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[derive(Debug, Serialize)]
pub struct GraphJson {
    pub name: String,
    pub nodes: Vec<NodeJson>,
    pub roads: Vec<RoadJson>,
}

#[derive(Debug, Serialize)]
pub struct NodeJson {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Serialize)]
pub struct RoadJson {
    pub from: String,
    pub to: String,
    pub road: String,
    pub distance_km: f64,
    pub speed_kmh: f64,
    pub two_way: bool,
}

impl GraphJson {
    pub fn new(graph: &Graph) -> Self {
        Self {
            name: graph.name().to_string(),
            nodes: graph
                .nodes()
                .iter()
                .map(|n| NodeJson {
                    id: n.id.clone(),
                    name: n.name.clone(),
                    lat: n.lat,
                    lon: n.lon,
                })
                .collect(),
            roads: graph
                .roads()
                .iter()
                .map(|r| RoadJson {
                    from: graph.node(r.from).id.clone(),
                    to: graph.node(r.to).id.clone(),
                    road: r.name.clone(),
                    distance_km: round1(r.distance_km),
                    speed_kmh: r.speed_kmh,
                    two_way: r.two_way,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RouteJson {
    pub found: bool,
    pub algorithm: &'static str,
    pub metric: &'static str,
    pub path: Vec<String>,
    pub legs: Vec<LegJson>,
    pub distance_km: f64,
    pub time_min: f64,
    pub nodes_expanded: usize,
    pub expansion_order: Vec<String>,
    pub elapsed_us: f64,
}

#[derive(Debug, Serialize)]
pub struct LegJson {
    pub from: String,
    pub to: String,
    pub road: String,
    pub distance_km: f64,
    pub time_min: f64,
}

impl RouteJson {
    pub fn new(graph: &Graph, result: &RouteResult) -> Self {
        let id = |node| graph.node(node).id.clone();
        let (path, legs, distance_km, time_min) = match &result.route {
            Some(route) => (
                route.path.iter().copied().map(id).collect(),
                route
                    .legs
                    .iter()
                    .map(|leg| LegJson {
                        from: id(leg.from),
                        to: id(leg.to),
                        road: graph.roads()[leg.road].name.clone(),
                        distance_km: round1(leg.distance_km),
                        time_min: round1(leg.time_min),
                    })
                    .collect(),
                round1(route.distance_km),
                round1(route.time_min),
            ),
            None => (Vec::new(), Vec::new(), 0.0, 0.0),
        };

        Self {
            found: result.found(),
            algorithm: result.algorithm.key(),
            metric: result.metric.key(),
            path,
            legs,
            distance_km,
            time_min,
            nodes_expanded: result.stats.nodes_expanded(),
            expansion_order: result
                .stats
                .expansion_order
                .iter()
                .copied()
                .map(id)
                .collect(),
            elapsed_us: round1(result.stats.elapsed.as_secs_f64() * 1e6),
        }
    }
}
