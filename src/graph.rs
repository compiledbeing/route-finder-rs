//! The road network: locations, roads and the adjacency list used by the search algorithms.

use std::collections::HashMap;

use thiserror::Error;

/// Index of a location inside a [`Graph`].
pub type NodeId = usize;

/// A location (intersection, city, ...) on the map.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Stable key used in the data file, e.g. `"bandung"`.
    pub id: String,
    /// Display name, e.g. `"Bandung"`.
    pub name: String,
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
}

/// A road as described in the data file (one entry, even when it is two-way).
#[derive(Debug, Clone, PartialEq)]
pub struct Road {
    pub from: NodeId,
    pub to: NodeId,
    pub distance_km: f64,
    pub speed_kmh: f64,
    pub name: String,
    pub two_way: bool,
}

/// One directed arc in the adjacency list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    pub to: NodeId,
    pub distance_km: f64,
    pub speed_kmh: f64,
    /// Index into [`Graph::roads`].
    pub road: usize,
}

impl Edge {
    /// Travel time in minutes at the road's speed.
    pub fn time_min(&self) -> f64 {
        self.distance_km / self.speed_kmh * 60.0
    }
}

/// Great-circle distance between two locations in kilometres.
pub fn haversine_km(a: &Node, b: &Node) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0088;

    let (lat_a, lat_b) = (a.lat.to_radians(), b.lat.to_radians());
    let d_lat = lat_b - lat_a;
    let d_lon = (b.lon - a.lon).to_radians();
    let h = (d_lat / 2.0).sin().powi(2) + lat_a.cos() * lat_b.cos() * (d_lon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().min(1.0).asin()
}

/// Reasons a location or road can be rejected.
#[derive(Debug, Error, PartialEq)]
pub enum GraphError {
    #[error("duplicate location id: {0}")]
    DuplicateId(String),
    #[error("road refers to a location that does not exist (index {0})")]
    UnknownNode(NodeId),
    #[error("road connects {0} to itself")]
    SelfLoop(String),
    #[error("road {from} - {to} needs a positive speed")]
    InvalidSpeed { from: String, to: String },
    #[error(
        "road {from} - {to} is {distance_km:.1} km, shorter than the {straight_km:.1} km straight line"
    )]
    ShorterThanStraightLine {
        from: String,
        to: String,
        distance_km: f64,
        straight_km: f64,
    },
}

/// A weighted graph stored as an adjacency list.
///
/// **Invariant:** every road is at least as long as the straight line between its
/// endpoints. That is what makes the haversine heuristic used by A* admissible, so
/// [`Graph::add_road`] rejects roads that would break it.
#[derive(Debug, Clone)]
pub struct Graph {
    name: String,
    nodes: Vec<Node>,
    adjacency: Vec<Vec<Edge>>,
    roads: Vec<Road>,
    /// Lower-cased id and display name -> node.
    lookup: HashMap<String, NodeId>,
    max_speed_kmh: f64,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    pub fn new() -> Self {
        Self {
            name: "Untitled network".to_string(),
            nodes: Vec::new(),
            adjacency: Vec::new(),
            roads: Vec::new(),
            lookup: HashMap::new(),
            max_speed_kmh: 0.0,
        }
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Adds a location and returns its id. Ids are matched case-insensitively.
    pub fn add_node(&mut self, node: Node) -> Result<NodeId, GraphError> {
        let id_key = node.id.to_lowercase();
        if self.lookup.contains_key(&id_key) {
            return Err(GraphError::DuplicateId(node.id));
        }
        let index = self.nodes.len();
        self.lookup.insert(id_key, index);
        // If two display names collide, the first location keeps the name.
        self.lookup.entry(node.name.to_lowercase()).or_insert(index);
        self.nodes.push(node);
        self.adjacency.push(Vec::new());
        Ok(index)
    }

    /// Adds a road between two existing locations.
    pub fn add_road(
        &mut self,
        from: NodeId,
        to: NodeId,
        distance_km: f64,
        speed_kmh: f64,
        name: impl Into<String>,
        two_way: bool,
    ) -> Result<(), GraphError> {
        let (a, b) = match (self.nodes.get(from), self.nodes.get(to)) {
            (Some(a), Some(b)) => (a, b),
            (None, _) => return Err(GraphError::UnknownNode(from)),
            (_, None) => return Err(GraphError::UnknownNode(to)),
        };
        if from == to {
            return Err(GraphError::SelfLoop(a.name.clone()));
        }
        if speed_kmh.is_nan() || speed_kmh <= 0.0 {
            return Err(GraphError::InvalidSpeed {
                from: a.name.clone(),
                to: b.name.clone(),
            });
        }
        let straight_km = haversine_km(a, b);
        if distance_km.is_nan() || distance_km < straight_km - 1e-6 {
            return Err(GraphError::ShorterThanStraightLine {
                from: a.name.clone(),
                to: b.name.clone(),
                distance_km,
                straight_km,
            });
        }

        let road = self.roads.len();
        self.roads.push(Road {
            from,
            to,
            distance_km,
            speed_kmh,
            name: name.into(),
            two_way,
        });
        self.adjacency[from].push(Edge {
            to,
            distance_km,
            speed_kmh,
            road,
        });
        if two_way {
            self.adjacency[to].push(Edge {
                to: from,
                distance_km,
                speed_kmh,
                road,
            });
        }
        self.max_speed_kmh = self.max_speed_kmh.max(speed_kmh);
        Ok(())
    }

    /// Looks a location up by id or display name, case-insensitively.
    pub fn find(&self, query: &str) -> Option<NodeId> {
        self.lookup.get(&query.to_lowercase()).copied()
    }

    /// The location with the given id. Panics if `id` is out of range.
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    /// Roads leaving `id`. Panics if `id` is out of range.
    pub fn neighbors(&self, id: NodeId) -> &[Edge] {
        &self.adjacency[id]
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn roads(&self) -> &[Road] {
        &self.roads
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn road_count(&self) -> usize {
        self.roads.len()
    }

    /// The fastest speed of any road; the time heuristic divides by this.
    pub fn max_speed_kmh(&self) -> f64 {
        self.max_speed_kmh
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, lat: f64, lon: f64) -> Node {
        Node {
            id: id.into(),
            name: id.to_uppercase(),
            lat,
            lon,
        }
    }

    #[test]
    fn one_degree_of_longitude_on_the_equator_is_about_111_km() {
        let d = haversine_km(&node("a", 0.0, 0.0), &node("b", 0.0, 1.0));
        assert!((d - 111.195).abs() < 0.01, "got {d}");
    }

    #[test]
    fn distance_to_self_is_zero() {
        let a = node("a", -6.9, 107.6);
        assert_eq!(haversine_km(&a, &a), 0.0);
    }

    #[test]
    fn lookup_is_case_insensitive_and_matches_ids_or_names() {
        let mut g = Graph::new();
        g.add_node(node("bandung", -6.9, 107.6)).unwrap();
        assert_eq!(g.find("BANDUNG"), Some(0));
        assert_eq!(g.find("Bandung"), Some(0));
        assert_eq!(g.find("nowhere"), None);
    }

    #[test]
    fn rejects_duplicate_ids_ignoring_case() {
        let mut g = Graph::new();
        g.add_node(node("a", 0.0, 0.0)).unwrap();
        assert_eq!(
            g.add_node(node("A", 1.0, 1.0)),
            Err(GraphError::DuplicateId("A".into()))
        );
    }

    #[test]
    fn rejects_roads_shorter_than_the_straight_line() {
        let mut g = Graph::new();
        let a = g.add_node(node("a", 0.0, 0.0)).unwrap();
        let b = g.add_node(node("b", 0.0, 1.0)).unwrap();
        let err = g
            .add_road(a, b, 50.0, 60.0, "teleporter", true)
            .unwrap_err();
        assert!(matches!(err, GraphError::ShorterThanStraightLine { .. }));
        assert!(g.add_road(a, b, 120.0, 60.0, "ok", true).is_ok());
    }

    #[test]
    fn rejects_bad_speeds_self_loops_and_unknown_nodes() {
        let mut g = Graph::new();
        let a = g.add_node(node("a", 0.0, 0.0)).unwrap();
        let b = g.add_node(node("b", 0.0, 1.0)).unwrap();
        assert!(matches!(
            g.add_road(a, b, 200.0, 0.0, "x", true),
            Err(GraphError::InvalidSpeed { .. })
        ));
        assert!(matches!(
            g.add_road(a, b, 200.0, f64::NAN, "x", true),
            Err(GraphError::InvalidSpeed { .. })
        ));
        assert!(matches!(
            g.add_road(a, a, 10.0, 50.0, "x", true),
            Err(GraphError::SelfLoop(_))
        ));
        assert_eq!(
            g.add_road(a, 9, 10.0, 50.0, "x", true),
            Err(GraphError::UnknownNode(9))
        );
    }

    #[test]
    fn one_way_roads_only_create_one_arc() {
        let mut g = Graph::new();
        let a = g.add_node(node("a", 0.0, 0.0)).unwrap();
        let b = g.add_node(node("b", 0.0, 1.0)).unwrap();
        g.add_road(a, b, 120.0, 50.0, "one-way", false).unwrap();
        assert_eq!(g.neighbors(a).len(), 1);
        assert!(g.neighbors(b).is_empty());
        assert_eq!(g.road_count(), 1);
    }
}
