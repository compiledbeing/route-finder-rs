//! Route search: breadth-first search, Dijkstra's algorithm and A*.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};
use std::fmt;
use std::str::FromStr;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::graph::{haversine_km, Edge, Graph, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Algorithm {
    /// Fewest stops; ignores road length and speed.
    Bfs,
    /// Cheapest route by distance or time.
    Dijkstra,
    /// Same optimal answer as Dijkstra, guided by a straight-line estimate.
    AStar,
}

impl Algorithm {
    pub const ALL: [Algorithm; 3] = [Algorithm::Bfs, Algorithm::Dijkstra, Algorithm::AStar];

    /// Machine-friendly name: `bfs`, `dijkstra`, `astar`.
    pub fn key(self) -> &'static str {
        match self {
            Algorithm::Bfs => "bfs",
            Algorithm::Dijkstra => "dijkstra",
            Algorithm::AStar => "astar",
        }
    }

    /// Display name: `BFS`, `Dijkstra`, `A*`.
    pub fn label(self) -> &'static str {
        match self {
            Algorithm::Bfs => "BFS",
            Algorithm::Dijkstra => "Dijkstra",
            Algorithm::AStar => "A*",
        }
    }
}

impl fmt::Display for Algorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("unknown algorithm '{0}' (use bfs, dijkstra or astar)")]
pub struct ParseAlgorithmError(pub String);

impl FromStr for Algorithm {
    type Err = ParseAlgorithmError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bfs" => Ok(Algorithm::Bfs),
            "dijkstra" => Ok(Algorithm::Dijkstra),
            "astar" | "a*" | "a-star" => Ok(Algorithm::AStar),
            _ => Err(ParseAlgorithmError(s.to_string())),
        }
    }
}

/// What the weighted algorithms minimise. BFS ignores it (it minimises stops).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Metric {
    Distance,
    Time,
}

impl Metric {
    pub fn key(self) -> &'static str {
        match self {
            Metric::Distance => "distance",
            Metric::Time => "time",
        }
    }

    /// Human-readable goal, e.g. for report headings.
    pub fn goal(self) -> &'static str {
        match self {
            Metric::Distance => "shortest distance",
            Metric::Time => "fastest time",
        }
    }
}

impl fmt::Display for Metric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("unknown metric '{0}' (use distance or time)")]
pub struct ParseMetricError(pub String);

impl FromStr for Metric {
    type Err = ParseMetricError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "distance" | "shortest" => Ok(Metric::Distance),
            "time" | "fastest" => Ok(Metric::Time),
            _ => Err(ParseMetricError(s.to_string())),
        }
    }
}

/// One hop of a route.
#[derive(Debug, Clone, PartialEq)]
pub struct Leg {
    pub from: NodeId,
    pub to: NodeId,
    pub distance_km: f64,
    pub time_min: f64,
    /// Index into [`Graph::roads`].
    pub road: usize,
}

/// A complete route from the start to the destination.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// Every stop, start first. A route to yourself has a single stop.
    pub path: Vec<NodeId>,
    /// `path.len() - 1` hops.
    pub legs: Vec<Leg>,
    pub distance_km: f64,
    pub time_min: f64,
}

impl Route {
    /// The total this route would be judged on under `metric`.
    pub fn cost(&self, metric: Metric) -> f64 {
        match metric {
            Metric::Distance => self.distance_km,
            Metric::Time => self.time_min,
        }
    }
}

/// How hard the search worked; handy for comparing algorithms.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchStats {
    /// Locations in the order the search settled them (used to animate the search).
    pub expansion_order: Vec<NodeId>,
    pub elapsed: Duration,
}

impl SearchStats {
    pub fn nodes_expanded(&self) -> usize {
        self.expansion_order.len()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteResult {
    pub algorithm: Algorithm,
    pub metric: Metric,
    /// `None` when the destination cannot be reached.
    pub route: Option<Route>,
    pub stats: SearchStats,
}

impl RouteResult {
    pub fn found(&self) -> bool {
        self.route.is_some()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RouteError {
    #[error("location id {0} is out of range")]
    NodeOutOfRange(NodeId),
}

/// Finds a route from `from` to `to`.
///
/// Returns `Ok` with `route: None` when no road connects the two locations, and
/// `Err` only when an id does not belong to the graph.
pub fn find_route(
    graph: &Graph,
    from: NodeId,
    to: NodeId,
    algorithm: Algorithm,
    metric: Metric,
) -> Result<RouteResult, RouteError> {
    for id in [from, to] {
        if id >= graph.node_count() {
            return Err(RouteError::NodeOutOfRange(id));
        }
    }

    let started = Instant::now();
    let (route, expansion_order) = match algorithm {
        Algorithm::Bfs => bfs(graph, from, to),
        Algorithm::Dijkstra => best_first(graph, from, to, metric, false),
        Algorithm::AStar => best_first(graph, from, to, metric, true),
    };

    Ok(RouteResult {
        algorithm,
        metric,
        route,
        stats: SearchStats {
            expansion_order,
            elapsed: started.elapsed(),
        },
    })
}

/// The predecessor of each node on the best known path, plus the edge used to get there.
type Predecessors = Vec<Option<(NodeId, Edge)>>;

/// Walks the predecessor links back from `dst` to `src`.
fn build_route(src: NodeId, dst: NodeId, prev: &Predecessors) -> Route {
    let mut legs = Vec::new();
    let mut node = dst;
    // `src` has no predecessor, so the loop ends there.
    while let Some((parent, edge)) = prev[node] {
        legs.push(Leg {
            from: parent,
            to: node,
            distance_km: edge.distance_km,
            time_min: edge.time_min(),
            road: edge.road,
        });
        node = parent;
    }
    debug_assert_eq!(node, src, "predecessor chain must lead back to the start");
    legs.reverse();

    let path = std::iter::once(src)
        .chain(legs.iter().map(|leg| leg.to))
        .collect();
    let distance_km = legs.iter().map(|leg| leg.distance_km).sum();
    let time_min = legs.iter().map(|leg| leg.time_min).sum();
    Route {
        path,
        legs,
        distance_km,
        time_min,
    }
}

/// Breadth-first search: fewest stops, ignores edge weights. O(V + E).
fn bfs(graph: &Graph, src: NodeId, dst: NodeId) -> (Option<Route>, Vec<NodeId>) {
    let mut prev: Predecessors = vec![None; graph.node_count()];
    let mut seen = vec![false; graph.node_count()];
    let mut frontier = VecDeque::from([src]);
    let mut order = Vec::new();

    seen[src] = true;
    while let Some(u) = frontier.pop_front() {
        order.push(u);
        if u == dst {
            return (Some(build_route(src, dst, &prev)), order);
        }
        for &edge in graph.neighbors(u) {
            if !seen[edge.to] {
                seen[edge.to] = true;
                prev[edge.to] = Some((u, edge));
                frontier.push_back(edge.to);
            }
        }
    }
    (None, order)
}

/// An entry in the open set. `BinaryHeap` is a max-heap, so the ordering is reversed
/// to pop the *lowest* priority first; ties go to the lower node id so runs are
/// deterministic.
struct QueueItem {
    priority: f64,
    node: NodeId,
}

impl Ord for QueueItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority
            .total_cmp(&self.priority)
            .then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for QueueItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for QueueItem {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for QueueItem {}

fn edge_cost(edge: &Edge, metric: Metric) -> f64 {
    match metric {
        Metric::Distance => edge.distance_km,
        Metric::Time => edge.time_min(),
    }
}

/// Best-first search. With `use_heuristic == false` this is Dijkstra's algorithm
/// (priority = cost so far). With `true` it is A*: priority = cost so far plus a
/// straight-line estimate of the cost still to go. Both run in O((V + E) log V).
///
/// The estimate never exceeds the true remaining cost (roads are never shorter than
/// the straight line, and no road is faster than `max_speed_kmh`), so A* returns the
/// same optimal route as Dijkstra while expanding fewer nodes.
fn best_first(
    graph: &Graph,
    src: NodeId,
    dst: NodeId,
    metric: Metric,
    use_heuristic: bool,
) -> (Option<Route>, Vec<NodeId>) {
    let goal = graph.node(dst);
    let max_speed = graph.max_speed_kmh();
    let heuristic = |v: NodeId| -> f64 {
        if !use_heuristic {
            return 0.0;
        }
        let straight_km = haversine_km(graph.node(v), goal);
        match metric {
            Metric::Distance => straight_km,
            Metric::Time if max_speed > 0.0 => straight_km / max_speed * 60.0,
            Metric::Time => 0.0,
        }
    };

    let mut best = vec![f64::INFINITY; graph.node_count()];
    let mut prev: Predecessors = vec![None; graph.node_count()];
    let mut settled = vec![false; graph.node_count()];
    let mut open = BinaryHeap::new();
    let mut order = Vec::new();

    best[src] = 0.0;
    open.push(QueueItem {
        priority: heuristic(src),
        node: src,
    });
    while let Some(QueueItem { node: u, .. }) = open.pop() {
        if settled[u] {
            continue; // stale queue entry: a cheaper one was already processed
        }
        settled[u] = true;
        order.push(u);
        if u == dst {
            return (Some(build_route(src, dst, &prev)), order);
        }
        for &edge in graph.neighbors(u) {
            let candidate = best[u] + edge_cost(&edge, metric);
            if candidate < best[edge.to] {
                best[edge.to] = candidate;
                prev[edge.to] = Some((u, edge));
                open.push(QueueItem {
                    priority: candidate + heuristic(edge.to),
                    node: edge.to,
                });
            }
        }
    }
    (None, order)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn algorithm_names_round_trip() {
        for algorithm in Algorithm::ALL {
            assert_eq!(algorithm.key().parse::<Algorithm>(), Ok(algorithm));
        }
        assert_eq!("A*".parse::<Algorithm>(), Ok(Algorithm::AStar));
        assert!("magic".parse::<Algorithm>().is_err());
    }

    #[test]
    fn metric_names_parse() {
        assert_eq!("Distance".parse::<Metric>(), Ok(Metric::Distance));
        assert_eq!("fastest".parse::<Metric>(), Ok(Metric::Time));
        assert!("speed".parse::<Metric>().is_err());
    }

    #[test]
    fn queue_pops_lowest_priority_first_and_breaks_ties_by_node() {
        let mut heap = BinaryHeap::new();
        for (priority, node) in [(5.0, 1), (1.0, 7), (1.0, 3), (3.0, 2)] {
            heap.push(QueueItem { priority, node });
        }
        let popped: Vec<_> = std::iter::from_fn(|| heap.pop().map(|i| i.node)).collect();
        assert_eq!(popped, vec![3, 7, 2, 1]);
    }
}
