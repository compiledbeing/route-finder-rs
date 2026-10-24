//! Plain-text rendering of routes, comparisons and the location list.

use std::fmt::Write as _;

use crate::graph::Graph;
use crate::pathfinder::{Algorithm, RouteResult};

const RULE_WIDTH: usize = 60;

/// `4 h 02 min` for an hour or more, `45 min` otherwise.
pub fn format_duration(minutes: f64) -> String {
    let total = minutes.round() as u64;
    if total >= 60 {
        format!("{} h {:02} min", total / 60, total % 60)
    } else {
        format!("{total} min")
    }
}

/// `234.9 km`.
pub fn format_km(km: f64) -> String {
    format!("{km:.1} km")
}

fn elapsed_us(result: &RouteResult) -> f64 {
    result.stats.elapsed.as_secs_f64() * 1e6
}

/// A route as a stop-by-stop itinerary with totals and search statistics.
pub fn format_route(graph: &Graph, result: &RouteResult) -> String {
    let Some(route) = &result.route else {
        return format!("No route found ({}).\n", result.algorithm.label());
    };

    let goal = match result.algorithm {
        Algorithm::Bfs => "fewest stops",
        _ => result.metric.goal(),
    };
    let rule = "-".repeat(RULE_WIDTH);
    let start = graph.node(route.path[0]);
    let end = graph.node(route.path[route.path.len() - 1]);

    let mut out = String::new();
    writeln!(
        out,
        "{} -> {}   [{}, {goal}]",
        start.name,
        end.name,
        result.algorithm.label()
    )
    .unwrap();
    writeln!(out, "{rule}").unwrap();
    writeln!(out, "  o {}", start.name).unwrap();
    for leg in &route.legs {
        writeln!(
            out,
            "  |   {:>9}  {:>9}   {}",
            format_km(leg.distance_km),
            format_duration(leg.time_min),
            graph.roads()[leg.road].name
        )
        .unwrap();
        writeln!(out, "  o {}", graph.node(leg.to).name).unwrap();
    }
    writeln!(out, "{rule}").unwrap();
    writeln!(
        out,
        "Total    {}  |  {}  |  {} stops",
        format_km(route.distance_km),
        format_duration(route.time_min),
        route.path.len()
    )
    .unwrap();
    writeln!(
        out,
        "Search   {} nodes expanded in {:.1} us",
        result.stats.nodes_expanded(),
        elapsed_us(result)
    )
    .unwrap();
    out
}

/// A side-by-side table of several results for the same trip.
pub fn format_comparison(results: &[RouteResult]) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "{:<10}{:>12}{:>12}{:>8}{:>10}{:>11}",
        "Algorithm", "Distance", "Time", "Stops", "Expanded", "Time (us)"
    )
    .unwrap();
    for result in results {
        let (distance, time, stops) = match &result.route {
            Some(route) => (
                format_km(route.distance_km),
                format_duration(route.time_min),
                route.path.len().to_string(),
            ),
            None => ("-".into(), "-".into(), "-".into()),
        };
        writeln!(
            out,
            "{:<10}{:>12}{:>12}{:>8}{:>10}{:>11.1}",
            result.algorithm.label(),
            distance,
            time,
            stops,
            result.stats.nodes_expanded(),
            elapsed_us(result)
        )
        .unwrap();
    }
    out
}

/// Every location with its number of connections.
pub fn format_locations(graph: &Graph) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "{}: {} locations, {} roads\n",
        graph.name(),
        graph.node_count(),
        graph.road_count()
    )
    .unwrap();
    for (id, node) in graph.nodes().iter().enumerate() {
        let connections = graph.neighbors(id).len();
        let noun = if connections == 1 {
            "connection"
        } else {
            "connections"
        };
        writeln!(
            out,
            "  {:<14}{:<14}{connections} {noun}",
            node.id, node.name
        )
        .unwrap();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(0.0), "0 min");
        assert_eq!(format_duration(45.4), "45 min");
        assert_eq!(format_duration(60.0), "1 h 00 min");
        assert_eq!(format_duration(282.0), "4 h 42 min");
    }

    #[test]
    fn formats_distances_with_one_decimal() {
        assert_eq!(format_km(234.94), "234.9 km");
        assert_eq!(format_km(0.0), "0.0 km");
    }
}
