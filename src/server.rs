//! The web UI and JSON API behind `route-finder serve`.
//!
//! * `GET /`                 the interactive map (embedded in the binary)
//! * `GET /api/graph`        all locations and roads
//! * `GET /api/route?from=&to=&algo=bfs|dijkstra|astar&metric=distance|time`

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::export::{GraphJson, RouteJson};
use crate::graph::Graph;
use crate::pathfinder::{find_route, Algorithm, Metric};

const INDEX_HTML: &str = include_str!("../web/index.html");

struct AppState {
    graph: Graph,
    /// The network never changes while the server runs, so it is serialised once.
    graph_json: Bytes,
}

/// Builds the application router. Exposed so tests can call it without a socket.
pub fn router(graph: Graph) -> Router {
    let graph_json = serde_json::to_vec(&GraphJson::new(&graph))
        .expect("graph serialisation cannot fail")
        .into();
    let state = Arc::new(AppState { graph, graph_json });

    Router::new()
        .route("/", get(index))
        .route("/api/graph", get(graph_handler))
        .route("/api/route", get(route_handler))
        .with_state(state)
}

/// Serves on `127.0.0.1:port` until Ctrl+C.
pub async fn serve(graph: Graph, port: u16) -> std::io::Result<()> {
    let name = graph.name().to_string();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    println!("Serving {name} at http://127.0.0.1:{port}  (Ctrl+C to stop)");
    axum::serve(listener, router(graph))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn graph_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json")],
        state.graph_json.clone(),
    )
}

#[derive(Deserialize)]
struct RouteParams {
    from: Option<String>,
    to: Option<String>,
    algo: Option<String>,
    metric: Option<String>,
}

async fn route_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RouteParams>,
) -> Result<Json<RouteJson>, ApiError> {
    let graph = &state.graph;
    let (Some(from_query), Some(to_query)) = (params.from.as_deref(), params.to.as_deref()) else {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Query parameters 'from' and 'to' are required.",
        ));
    };

    let locate = |query: &str| {
        graph.find(query).ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, format!("Unknown location: {query}"))
        })
    };
    let from = locate(from_query)?;
    let to = locate(to_query)?;

    let algorithm = params
        .algo
        .as_deref()
        .unwrap_or("astar")
        .parse::<Algorithm>()
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.to_string()))?;
    let metric = params
        .metric
        .as_deref()
        .unwrap_or("distance")
        .parse::<Metric>()
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.to_string()))?;

    let result = find_route(graph, from, to, algorithm, metric)
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(RouteJson::new(graph, &result)))
}

/// An error returned to API clients as `{"error": "..."}`.
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}
