use std::net::SocketAddr;
use std::time::Instant;

use axum::{
    extract::Json,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use rs_trafilatura::{extract_with_options, Options};

#[derive(Deserialize)]
#[allow(dead_code)]
struct ParseRequest {
    url: Option<String>,
    content: Option<String>,
    encoding: Option<String>,
    crawl_timestamp: Option<i64>,
    query: Option<String>,
    source: Option<String>,
    img_in_content: Option<bool>,
    output_format: Option<String>,
}

#[derive(Serialize)]
struct ParseResponse {
    ret_code: i32,
    content: String,
    url: String,
    title: String,
    head_title: String,
    publish_time: String,
    image_list: Vec<String>,
    video_list: Vec<String>,
    position_list: Vec<i32>,
    time_cost: i64,
    should_cache: bool,
    hostname: String,
    hostlogo: String,
}

fn error_response(ret_code: i32, url: String, time_cost: i64) -> ParseResponse {
    ParseResponse {
        ret_code,
        content: String::new(),
        url,
        title: String::new(),
        head_title: String::new(),
        publish_time: String::new(),
        image_list: Vec::new(),
        video_list: Vec::new(),
        position_list: Vec::new(),
        time_cost,
        should_cache: false,
        hostname: String::new(),
        hostlogo: String::new(),
    }
}

async fn health() -> &'static str {
    "OK"
}

async fn parse(Json(req): Json<ParseRequest>) -> (StatusCode, Json<ParseResponse>) {
    let start = Instant::now();

    let html = match req.content.as_deref() {
        Some(c) if !c.is_empty() => c.to_string(),
        _ => {
            let elapsed = start.elapsed().as_millis() as i64;
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response(3, req.url.unwrap_or_default(), elapsed)),
            );
        }
    };

    let output_markdown = matches!(req.output_format.as_deref(), Some("markdown") | Some("0"));
    let include_images = req.img_in_content.unwrap_or(false);

    let options = Options {
        url: req.url.clone(),
        include_images,
        output_markdown,
        ..Options::default()
    };

    match extract_with_options(&html, &options) {
        Ok(result) => {
            let elapsed = start.elapsed().as_millis() as i64;

            let content = match req.output_format.as_deref() {
                Some("markdown") | Some("0") => {
                    result.content_markdown.unwrap_or(result.content_text)
                }
                Some("html") | Some("2") => {
                    result.content_html.unwrap_or(result.content_text)
                }
                _ => result.content_text,
            };

            let title = result.metadata.title.clone().unwrap_or_default();
            let publish_time = result
                .metadata
                .date
                .map(|d| d.to_rfc3339())
                .unwrap_or_default();

            (
                StatusCode::OK,
                Json(ParseResponse {
                    ret_code: 0,
                    content,
                    url: req.url.unwrap_or_default(),
                    title: title.clone(),
                    head_title: title,
                    publish_time,
                    image_list: result.images.iter().map(|img| img.src.clone()).collect(),
                    video_list: Vec::new(),
                    position_list: Vec::new(),
                    time_cost: elapsed,
                    should_cache: result.extraction_quality > 0.8,
                    hostname: result.metadata.hostname.unwrap_or_default(),
                    hostlogo: result.metadata.image.unwrap_or_default(),
                }),
            )
        }
        Err(e) => {
            let elapsed = start.elapsed().as_millis() as i64;
            let ret_code = match e {
                rs_trafilatura::Error::ParseError(_) => 1,
                rs_trafilatura::Error::NoContent => 3,
                _ => 2,
            };
            (
                StatusCode::OK,
                Json(error_response(ret_code, req.url.unwrap_or_default(), elapsed)),
            )
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/health", get(health))
        .route("/parse", post(parse))
        .layer(CorsLayer::permissive());

    let host = std::env::var("WEBPARSER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("WEBPARSER_PORT").unwrap_or_else(|_| "3021".to_string());
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    tracing::info!("webparser HTTP server starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
