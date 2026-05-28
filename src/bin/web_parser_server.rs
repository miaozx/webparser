use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Mutex;
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

fn cleanup_old_logs(dir: &PathBuf, days: u64) {
    let cutoff = chrono::Duration::days(days as i64);
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "log") { continue; }
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if let Ok(age) = modified.elapsed() {
                        if chrono::Duration::from_std(age).unwrap_or_default() > cutoff {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }
    }
}

fn log_path(log_dir: &PathBuf) -> PathBuf {
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    log_dir.join(format!("extract.{date}.log"))
}

struct LogWriter {
    dir: PathBuf,
    date: Mutex<String>,
    file: Mutex<std::fs::File>,
}

impl LogWriter {
    fn new(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let path = log_path(&dir);
        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path).expect("open log");
        Self { dir, date: Mutex::new(date), file: Mutex::new(file) }
    }

    fn rotate(&self) -> bool {
        let now = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut date = self.date.lock().unwrap();
        if *date == now { return false; }
        *date = now;
        let path = log_path(&self.dir);
        *self.file.lock().unwrap() = std::fs::OpenOptions::new().create(true).append(true).open(&path).expect("open log");
        cleanup_old_logs(&self.dir, 30);
        true
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogWriter {
    type Writer = LogFile;

    fn make_writer(&'a self) -> Self::Writer {
        self.rotate();
        let file = self.file.lock().unwrap().try_clone().expect("clone file");
        LogFile(file)
    }
}

struct LogFile(std::fs::File);

impl Write for LogFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> { self.0.write(buf) }
    fn flush(&mut self) -> std::io::Result<()> { self.0.flush() }
}

fn init_log() {
    let log_dir = std::env::var("WEBPARSER_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".local").join("share").join("webparser").join("logs")
        });

    let writer = LogWriter::new(log_dir);

    tracing_subscriber::fmt()
        .with_target(true)
        .with_file(false)
        .with_line_number(false)
        .with_writer(writer)
        .with_env_filter("extract=info")
        .init();
}

fn worker_threads() -> usize {
    std::env::var("WEBPARSER_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_log();

    let n = worker_threads();
    tracing::info!("starting with {} worker threads", n);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(n)
        .enable_all()
        .build()?;
    rt.block_on(async { run_server().await })
}

async fn run_server() -> Result<(), Box<dyn std::error::Error>> {

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
