//! # route-finder
//!
//! A small routing engine: load a road network from JSON, then find routes with
//! breadth-first search, Dijkstra's algorithm or A*.
//!
//! ```
//! use route_finder::{find_route, parse_graph, Algorithm, Metric, DEFAULT_DATASET};
//!
//! let graph = parse_graph(DEFAULT_DATASET)?;
//! let from = graph.find("Bandung").unwrap();
//! let to = graph.find("Purwokerto").unwrap();
//!
//! let result = find_route(&graph, from, to, Algorithm::AStar, Metric::Distance)?;
//! let route = result.route.expect("the demo map is connected");
//! assert_eq!(route.path.len(), 6);
//! println!("{:.1} km via {} stops", route.distance_km, route.path.len());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod export;
pub mod graph;
pub mod loader;
pub mod pathfinder;
pub mod report;
#[cfg(feature = "server")]
pub mod server;

pub use graph::{haversine_km, Edge, Graph, GraphError, Node, NodeId, Road};
pub use loader::{load_graph, parse_graph, LoadError};
pub use pathfinder::{
    find_route, Algorithm, Leg, Metric, Route, RouteError, RouteResult, SearchStats,
};

/// The bundled demo network (`data/java_island.json`), compiled into the binary.
pub const DEFAULT_DATASET: &str = include_str!("../data/java_island.json");
