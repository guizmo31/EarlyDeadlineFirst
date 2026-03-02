use actix_cors::Cors;
use actix_files::Files;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, middleware};
use edf_core::{SchedulerConfig, simulate};
use std::path::PathBuf;

async fn api_simulate(body: web::Json<SchedulerConfig>) -> HttpResponse {
    let result = simulate(&body);
    HttpResponse::Ok().json(result)
}

async fn api_health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

async fn redirect_root(_req: HttpRequest) -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", "/simulator/"))
        .finish()
}

fn find_dir_with_index(candidates: &[Option<PathBuf>], fallback: &str) -> PathBuf {
    for candidate in candidates.iter().flatten() {
        if candidate.join("index.html").exists() {
            return candidate.clone();
        }
    }
    PathBuf::from(fallback)
}

fn find_web_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    find_dir_with_index(&[
        exe_dir.as_ref().map(|d| d.join("../../edf-web")),
        exe_dir.as_ref().map(|d| d.join("../../../edf-web")),
        Some(PathBuf::from("./edf-web")),
        Some(PathBuf::from("../edf-web")),
    ], "./edf-web")
}

fn find_viewer_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    find_dir_with_index(&[
        exe_dir.as_ref().map(|d| d.join("../../../edf-viewer")),
        Some(PathBuf::from("./edf-viewer")),
        Some(PathBuf::from("../edf-viewer")),
    ], "./edf-viewer")
}

fn find_builder_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    find_dir_with_index(&[
        exe_dir.as_ref().map(|d| d.join("../../../edf-builder")),
        Some(PathBuf::from("./edf-builder")),
        Some(PathBuf::from("../edf-builder")),
    ], "./edf-builder")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let web_dir = find_web_dir();
    let viewer_dir = find_viewer_dir();
    let builder_dir = find_builder_dir();
    println!("EDF Scheduler Server starting on http://localhost:8080");
    println!("  /simulator -> {}", web_dir.display());
    println!("  /builder   -> {}", builder_dir.display());
    println!("  /viewer    -> {}", viewer_dir.display());

    let web_dir_str = web_dir.to_string_lossy().to_string();
    let viewer_dir_str = viewer_dir.to_string_lossy().to_string();
    let builder_dir_str = builder_dir.to_string_lossy().to_string();

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .route("/", web::get().to(redirect_root))
            .route("/api/health", web::get().to(api_health))
            .route("/api/simulate", web::post().to(api_simulate))
            .service(Files::new("/builder", &builder_dir_str).index_file("index.html").redirect_to_slash_directory())
            .service(Files::new("/viewer", &viewer_dir_str).index_file("index.html").redirect_to_slash_directory())
            .service(Files::new("/simulator", &web_dir_str).index_file("index.html").redirect_to_slash_directory())
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
