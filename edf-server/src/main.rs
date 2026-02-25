use actix_cors::Cors;
use actix_files::Files;
use actix_web::{web, App, HttpResponse, HttpServer, middleware};
use edf_core::{SchedulerConfig, simulate};
use std::path::PathBuf;

async fn api_simulate(body: web::Json<SchedulerConfig>) -> HttpResponse {
    let result = simulate(&body);
    HttpResponse::Ok().json(result)
}

async fn api_health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

fn find_web_dir() -> PathBuf {
    // Try relative to executable first, then current dir
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    let candidates = [
        exe_dir.as_ref().map(|d| d.join("../../edf-web")),
        exe_dir.as_ref().map(|d| d.join("../../../edf-web")),
        Some(PathBuf::from("./edf-web")),
        Some(PathBuf::from("../edf-web")),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.join("index.html").exists() {
            return candidate.clone();
        }
    }

    PathBuf::from("./edf-web")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let web_dir = find_web_dir();
    println!("EDF Scheduler Server starting on http://localhost:8080");
    println!("Serving web files from: {}", web_dir.display());

    let web_dir_str = web_dir.to_string_lossy().to_string();

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .route("/api/health", web::get().to(api_health))
            .route("/api/simulate", web::post().to(api_simulate))
            .service(Files::new("/", &web_dir_str).index_file("index.html"))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
