//! Behavioural tests for the search algorithms, on a tiny hand-built graph and on the
//! bundled dataset.

use route_finder::{
    find_route, parse_graph, Algorithm, Graph, Metric, Node, NodeId, RouteError, DEFAULT_DATASET,
};

fn node(id: &str, lat: f64, lon: f64) -> Node {
    Node {
        id: id.into(),
        name: id.to_uppercase(),
        lat,
        lon,
    }
}

/// A -- B -- C along the equator (1 degree is ~111 km), plus a faster but longer A -- C road.
///
/// * A-B and B-C: 120 km each at 40 km/h  -> 240 km, 360 min via B
/// * A-C:         300 km at 100 km/h      -> 300 km, 180 min directly
///
/// So BFS and "fastest" go direct, while "shortest" goes via B. `D` is unreachable.
fn triangle() -> Graph {
    let mut g = Graph::new();
    let a = g.add_node(node("a", 0.0, 0.0)).unwrap();
    let b = g.add_node(node("b", 0.0, 1.0)).unwrap();
    let c = g.add_node(node("c", 0.0, 2.0)).unwrap();
    g.add_node(node("island", 10.0, 10.0)).unwrap();
    g.add_road(a, b, 120.0, 40.0, "AB", true).unwrap();
    g.add_road(b, c, 120.0, 40.0, "BC", true).unwrap();
    g.add_road(a, c, 300.0, 100.0, "AC", true).unwrap();
    g
}

fn path_of(
    g: &Graph,
    algorithm: Algorithm,
    metric: Metric,
    from: NodeId,
    to: NodeId,
) -> Vec<NodeId> {
    find_route(g, from, to, algorithm, metric)
        .unwrap()
        .route
        .expect("route exists")
        .path
}

#[test]
fn dijkstra_picks_the_shortest_route() {
    let g = triangle();
    let result = find_route(&g, 0, 2, Algorithm::Dijkstra, Metric::Distance).unwrap();
    let route = result.route.unwrap();
    assert_eq!(route.path, vec![0, 1, 2]);
    assert!((route.distance_km - 240.0).abs() < 1e-9);
    assert_eq!(route.legs.len(), 2);
}

#[test]
fn the_time_metric_prefers_the_faster_road() {
    let g = triangle();
    let route = find_route(&g, 0, 2, Algorithm::Dijkstra, Metric::Time)
        .unwrap()
        .route
        .unwrap();
    assert_eq!(route.path, vec![0, 2]);
    assert!((route.time_min - 180.0).abs() < 1e-9);
}

#[test]
fn bfs_minimises_stops_not_distance() {
    let g = triangle();
    assert_eq!(
        path_of(&g, Algorithm::Bfs, Metric::Distance, 0, 2),
        vec![0, 2]
    );
}

#[test]
fn astar_matches_dijkstra() {
    let g = triangle();
    for metric in [Metric::Distance, Metric::Time] {
        assert_eq!(
            path_of(&g, Algorithm::AStar, metric, 0, 2),
            path_of(&g, Algorithm::Dijkstra, metric, 0, 2)
        );
    }
}

#[test]
fn a_route_to_yourself_has_one_stop_and_no_legs() {
    let g = triangle();
    for algorithm in Algorithm::ALL {
        let route = find_route(&g, 1, 1, algorithm, Metric::Distance)
            .unwrap()
            .route
            .unwrap();
        assert_eq!(route.path, vec![1]);
        assert!(route.legs.is_empty());
        assert_eq!(route.distance_km, 0.0);
    }
}

#[test]
fn unreachable_destinations_yield_no_route() {
    let g = triangle();
    for algorithm in Algorithm::ALL {
        let result = find_route(&g, 0, 3, algorithm, Metric::Distance).unwrap();
        assert!(!result.found());
        assert!(
            result.stats.nodes_expanded() > 0,
            "the search still did work"
        );
    }
}

#[test]
fn invalid_ids_are_an_error_not_a_panic() {
    let g = triangle();
    assert_eq!(
        find_route(&g, 0, 99, Algorithm::AStar, Metric::Distance),
        Err(RouteError::NodeOutOfRange(99))
    );
}

#[test]
fn one_way_roads_are_respected() {
    let mut g = Graph::new();
    let a = g.add_node(node("a", 0.0, 0.0)).unwrap();
    let b = g.add_node(node("b", 0.0, 1.0)).unwrap();
    g.add_road(a, b, 120.0, 50.0, "one-way street", false)
        .unwrap();
    assert!(find_route(&g, a, b, Algorithm::Dijkstra, Metric::Distance)
        .unwrap()
        .found());
    assert!(!find_route(&g, b, a, Algorithm::Dijkstra, Metric::Distance)
        .unwrap()
        .found());
}

#[test]
fn legs_add_up_to_the_route_totals() {
    let g = parse_graph(DEFAULT_DATASET).unwrap();
    let (from, to) = (g.find("merak").unwrap(), g.find("banyuwangi").unwrap());
    for algorithm in Algorithm::ALL {
        let route = find_route(&g, from, to, algorithm, Metric::Time)
            .unwrap()
            .route
            .unwrap();
        assert_eq!(route.path.first(), Some(&from));
        assert_eq!(route.path.last(), Some(&to));
        assert_eq!(route.legs.len(), route.path.len() - 1);
        let km: f64 = route.legs.iter().map(|l| l.distance_km).sum();
        assert!((km - route.distance_km).abs() < 1e-9);
        // Consecutive legs must connect.
        assert!(route.legs.windows(2).all(|w| w[0].to == w[1].from));
    }
}

/// The key correctness property: for every pair of cities and both metrics,
/// A* and Dijkstra agree on the optimal cost, BFS never uses more stops, and A* does
/// less work overall.
#[test]
fn dijkstra_and_astar_agree_on_every_pair_of_the_real_dataset() {
    let g = parse_graph(DEFAULT_DATASET).unwrap();
    assert!(g.node_count() > 20);

    let (mut dijkstra_work, mut astar_work) = (0, 0);
    for metric in [Metric::Distance, Metric::Time] {
        for from in 0..g.node_count() {
            for to in 0..g.node_count() {
                let run = |algorithm| find_route(&g, from, to, algorithm, metric).unwrap();
                let (dijkstra, astar, bfs) = (
                    run(Algorithm::Dijkstra),
                    run(Algorithm::AStar),
                    run(Algorithm::Bfs),
                );

                let (d, a, b) = (
                    dijkstra.route.as_ref().expect("dataset is connected"),
                    astar.route.as_ref().expect("dataset is connected"),
                    bfs.route.as_ref().expect("dataset is connected"),
                );
                assert!(
                    (d.cost(metric) - a.cost(metric)).abs() < 1e-6,
                    "{} -> {} ({metric}): dijkstra {} vs astar {}",
                    g.node(from).name,
                    g.node(to).name,
                    d.cost(metric),
                    a.cost(metric)
                );
                assert!(b.path.len() <= d.path.len());

                dijkstra_work += dijkstra.stats.nodes_expanded();
                astar_work += astar.stats.nodes_expanded();
            }
        }
    }
    assert!(
        astar_work < dijkstra_work,
        "A* expanded {astar_work}, Dijkstra {dijkstra_work}"
    );
    println!("A* expanded {astar_work} nodes vs Dijkstra {dijkstra_work} across all pairs");
}
