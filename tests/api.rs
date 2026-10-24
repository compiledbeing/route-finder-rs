//! HTTP API tests. They call the router in-process, so no port is opened.
#![cfg(feature = "server")]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use route_finder::server::router;
use route_finder::{parse_graph, DEFAULT_DATASET};
use serde_json::Value;
use tower::ServiceExt;

async fn get(uri: &str) -> (StatusCode, Vec<u8>) {
    let app = router(parse_graph(DEFAULT_DATASET).unwrap());
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, body.to_vec())
}

async fn get_json(uri: &str) -> (StatusCode, Value) {
    let (status, body) = get(uri).await;
    (
        status,
        serde_json::from_slice(&body).expect("response is JSON"),
    )
}

#[tokio::test]
async fn serves_the_web_ui() {
    let (status, body) = get("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(String::from_utf8(body).unwrap().contains("Route Finder"));
}

#[tokio::test]
async fn graph_endpoint_lists_nodes_and_roads() {
    let (status, json) = get_json("/api/graph").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["nodes"].as_array().unwrap().len(), 34);
    assert_eq!(json["roads"].as_array().unwrap().len(), 49);
    assert_eq!(json["nodes"][0]["id"], "jakarta");
}

#[tokio::test]
async fn route_endpoint_returns_the_route_and_search_order() {
    let (status, json) =
        get_json("/api/route?from=Bandung&to=purwokerto&algo=astar&metric=distance").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["found"], true);
    assert_eq!(json["algorithm"], "astar");
    assert_eq!(json["path"].as_array().unwrap().len(), 6);
    assert_eq!(json["path"][0], "bandung");
    assert_eq!(json["legs"].as_array().unwrap().len(), 5);
    assert!((json["distance_km"].as_f64().unwrap() - 234.9).abs() < 0.1);
    assert_eq!(
        json["nodes_expanded"].as_u64().unwrap() as usize,
        json["expansion_order"].as_array().unwrap().len()
    );
}

#[tokio::test]
async fn algorithm_and_metric_default_to_astar_and_distance() {
    let (_, json) = get_json("/api/route?from=bandung&to=bogor").await;
    assert_eq!(json["algorithm"], "astar");
    assert_eq!(json["metric"], "distance");
}

#[tokio::test]
async fn bad_requests_get_a_json_error_and_the_right_status() {
    for (uri, status) in [
        ("/api/route?from=bandung", StatusCode::BAD_REQUEST),
        (
            "/api/route?from=bandung&to=bogor&algo=magic",
            StatusCode::BAD_REQUEST,
        ),
        (
            "/api/route?from=bandung&to=bogor&metric=comfort",
            StatusCode::BAD_REQUEST,
        ),
        ("/api/route?from=bandung&to=atlantis", StatusCode::NOT_FOUND),
    ] {
        let (got, json) = get_json(uri).await;
        assert_eq!(got, status, "{uri}");
        assert!(
            json["error"].as_str().is_some_and(|m| !m.is_empty()),
            "{uri}"
        );
    }
}
