use std::{io, net::SocketAddr};

use axum::{
    extract::{BodyStream, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use clamd_client::{ClamdClient, ClamdClientBuilder, ScanResult};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use tokio_util::io::StreamReader;
use tower_http::{
    trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{info, Level};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

#[derive(Clone)]
pub struct AppState {
    clamd_client: ClamdClient,
}

#[derive(Serialize, Deserialize)]
pub struct ScanResponse {
    virus: Option<Vec<String>>,
}

async fn scan(State(mut state): State<AppState>, stream: BodyStream) -> Result<Response, AppError> {
    let stream_with_io_err = stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
    let reader = StreamReader::new(stream_with_io_err);
    let result = state.clamd_client.scan_reader(reader).await?;
    let virus = match result {
        ScanResult::Benign => None,
        ScanResult::Malignent { infection_types } => Some(infection_types),
    };
    Ok(Json(ScanResponse { virus }).into_response())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "clamav_api_rs=info,tower_http=debug,axum::rejection=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app_state = AppState {
        clamd_client: ClamdClientBuilder::tcp_socket("clams:3310")?.build(),
    };

    let addr: SocketAddr = "0.0.0.0:4242".parse()?;
    let app = Router::new()
        .route("/scan", post(scan))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().include_headers(true))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Millis),
                ),
        )
        .with_state(app_state);
    info!(
        ip = addr.ip().to_string(),
        port = addr.port(),
        "starting server"
    );

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
