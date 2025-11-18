// Backend API server with embedded frontend
// TBM + TransGironde Transit API Server with integrated web UI

use actix_web::{web, App, HttpServer, HttpResponse, middleware, HttpRequest};
use actix_cors::Cors;
use actix_files as fs;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time;

mod tbm_api_models;
use tbm_api_models::{NVTModels, CachedNetworkData};

// Embed static files at compile time
const INDEX_HTML: &str = include_str!("../static/nvtweb.html");
const TRANSIT_JS: &str = include_str!("../static/tbm-transit.js");

#[derive(Clone)]
struct AppState {
    cache: Arc<Mutex<CachedNetworkData>>,
}

#[derive(Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
    timestamp: i64,
    sources: Vec<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn success(data: T) -> Self {
        ApiResponse {
            success: true,
            data: Some(data),
            error: None,
            timestamp: NVTModels::get_current_timestamp(),
            sources: vec!["TBM".to_string(), "TransGironde".to_string()],
        }
    }

    fn error(message: String) -> Self {
        ApiResponse {
            success: false,
            data: None,
            error: Some(message),
            timestamp: NVTModels::get_current_timestamp(),
            sources: vec![],
        }
    }
}

// ============================================================================
// Frontend Routes
// ============================================================================

async fn serve_index() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(INDEX_HTML)
}

async fn serve_js() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/javascript; charset=utf-8")
        .body(TRANSIT_JS)
}

// ============================================================================
// API Endpoints (keeping your existing ones)
// ============================================================================

async fn get_network_data(state: web::Data<AppState>) -> HttpResponse {
    match state.cache.lock() {
        Ok(cache) => {
            let network_data = cache.to_network_data();
            println!("📊 Network data requested: {} stops, {} lines, {} shapes",
                     network_data.stops.len(),
                     network_data.lines.len(),
                     network_data.shapes.len());
            HttpResponse::Ok().json(ApiResponse::success(network_data))
        }
        Err(e) => {
            eprintln!("❌ Failed to lock cache: {}", e);
            HttpResponse::InternalServerError()
                .json(ApiResponse::<String>::error(
                    "Failed to retrieve network data".to_string()
                ))
        }
    }
}

async fn get_stops(state: web::Data<AppState>) -> HttpResponse {
    match state.cache.lock() {
        Ok(cache) => {
            let network_data = cache.to_network_data();
            println!("📍 Stops requested: {} total", network_data.stops.len());
            HttpResponse::Ok().json(ApiResponse::success(network_data.stops))
        }
        Err(e) => {
            eprintln!("❌ Failed to lock cache: {}", e);
            HttpResponse::InternalServerError()
                .json(ApiResponse::<Vec<tbm_api_models::Stop>>::error(
                    "Failed to retrieve stops".to_string()
                ))
        }
    }
}

async fn get_lines(state: web::Data<AppState>) -> HttpResponse {
    match state.cache.lock() {
        Ok(cache) => {
            let network_data = cache.to_network_data();
            println!("🚌 Lines requested: {} total", network_data.lines.len());
            HttpResponse::Ok().json(ApiResponse::success(network_data.lines))
        }
        Err(e) => {
            eprintln!("❌ Failed to lock cache: {}", e);
            HttpResponse::InternalServerError()
                .json(ApiResponse::<Vec<tbm_api_models::Line>>::error(
                    "Failed to retrieve lines".to_string()
                ))
        }
    }
}

async fn get_vehicles(state: web::Data<AppState>) -> HttpResponse {
    match state.cache.lock() {
        Ok(cache) => {
            println!("🚗 Vehicles requested: {} active", cache.real_time.len());
            HttpResponse::Ok().json(ApiResponse::success(&cache.real_time))
        }
        Err(e) => {
            eprintln!("❌ Failed to lock cache: {}", e);
            HttpResponse::InternalServerError()
                .json(ApiResponse::<Vec<tbm_api_models::RealTimeInfo>>::error(
                    "Failed to retrieve vehicles".to_string()
                ))
        }
    }
}

async fn get_alerts(state: web::Data<AppState>) -> HttpResponse {
    match state.cache.lock() {
        Ok(cache) => {
            println!("⚠️  Alerts requested: {} active", cache.alerts.len());
            HttpResponse::Ok().json(ApiResponse::success(&cache.alerts))
        }
        Err(e) => {
            eprintln!("❌ Failed to lock cache: {}", e);
            HttpResponse::InternalServerError()
                .json(ApiResponse::<Vec<tbm_api_models::AlertInfo>>::error(
                    "Failed to retrieve alerts".to_string()
                ))
        }
    }
}

async fn get_stop_by_id(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let stop_id = path.into_inner();

    match state.cache.lock() {
        Ok(cache) => {
            let network_data = cache.to_network_data();
            match network_data.stops.iter().find(|s| s.stop_id == stop_id) {
                Some(stop) => {
                    println!("📍 Stop retrieved: {} ({})", stop.stop_name, stop.stop_id);
                    HttpResponse::Ok().json(ApiResponse::success(stop))
                }
                None => {
                    println!("⚠️  Stop not found: {}", stop_id);
                    HttpResponse::NotFound()
                        .json(ApiResponse::<String>::error(
                            format!("Stop '{}' not found", stop_id)
                        ))
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to lock cache: {}", e);
            HttpResponse::InternalServerError()
                .json(ApiResponse::<String>::error(
                    "Failed to retrieve stop".to_string()
                ))
        }
    }
}

async fn get_line_by_code(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let line_code = path.into_inner();

    match state.cache.lock() {
        Ok(cache) => {
            let network_data = cache.to_network_data();
            match network_data.lines.iter().find(|l|
                l.line_code.eq_ignore_ascii_case(&line_code)
            ) {
                Some(line) => {
                    println!("🚌 Line retrieved: {} ({}) - {}",
                             line.line_code, line.line_name, line.operator);
                    HttpResponse::Ok().json(ApiResponse::success(line))
                }
                None => {
                    println!("⚠️  Line not found: {}", line_code);
                    HttpResponse::NotFound()
                        .json(ApiResponse::<String>::error(
                            format!("Line '{}' not found", line_code)
                        ))
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to lock cache: {}", e);
            HttpResponse::InternalServerError()
                .json(ApiResponse::<String>::error(
                    "Failed to retrieve line".to_string()
                ))
        }
    }
}

async fn get_lines_by_operator(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let operator = path.into_inner();

    match state.cache.lock() {
        Ok(cache) => {
            let network_data = cache.to_network_data();
            let filtered_lines: Vec<_> = network_data.lines
                .into_iter()
                .filter(|l| l.operator.eq_ignore_ascii_case(&operator))
                .collect();

            if filtered_lines.is_empty() {
                println!("⚠️  No lines found for operator: {}", operator);
                HttpResponse::NotFound()
                    .json(ApiResponse::<Vec<tbm_api_models::Line>>::error(
                        format!("No lines found for operator '{}'", operator)
                    ))
            } else {
                println!("🚌 Lines retrieved for {}: {} lines", operator, filtered_lines.len());
                HttpResponse::Ok().json(ApiResponse::success(filtered_lines))
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to lock cache: {}", e);
            HttpResponse::InternalServerError()
                .json(ApiResponse::<Vec<tbm_api_models::Line>>::error(
                    "Failed to retrieve lines".to_string()
                ))
        }
    }
}

async fn get_stats(state: web::Data<AppState>) -> HttpResponse {
    match state.cache.lock() {
        Ok(cache) => {
            let stats = NVTModels::get_cache_stats(&cache);
            println!("📊 Stats requested");
            HttpResponse::Ok().json(ApiResponse::success(stats))
        }
        Err(e) => {
            eprintln!("❌ Failed to lock cache: {}", e);
            HttpResponse::InternalServerError()
                .json(ApiResponse::<String>::error(
                    "Failed to retrieve stats".to_string()
                ))
        }
    }
}

async fn get_operators(state: web::Data<AppState>) -> HttpResponse {
    match state.cache.lock() {
        Ok(cache) => {
            let network_data = cache.to_network_data();

            let mut operators = std::collections::HashMap::new();
            for line in &network_data.lines {
                *operators.entry(line.operator.clone()).or_insert(0) += 1;
            }

            let operator_info: Vec<_> = operators.iter()
                .map(|(name, count)| {
                    serde_json::json!({
                        "name": name,
                        "lines_count": count
                    })
                })
                .collect();

            println!("🏢 Operators requested: {} operators", operator_info.len());
            HttpResponse::Ok().json(ApiResponse::success(operator_info))
        }
        Err(e) => {
            eprintln!("❌ Failed to lock cache: {}", e);
            HttpResponse::InternalServerError()
                .json(ApiResponse::<String>::error(
                    "Failed to retrieve operators".to_string()
                ))
        }
    }
}

async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "TBM + TransGironde Transit API",
        "version": "1.1.0",
        "sources": ["TBM", "TransGironde"],
        "timestamp": NVTModels::get_current_timestamp(),
        "embedded_frontend": true
    }))
}

async fn force_refresh(state: web::Data<AppState>) -> HttpResponse {
    println!("🔄 Manual refresh requested...");

    let state_clone = state.cache.clone();
    match tokio::task::spawn_blocking(move || {
        match state_clone.lock() {
            Ok(mut cache) => NVTModels::smart_refresh(&mut cache),
            Err(e) => Err(tbm_api_models::NVTError::NetworkError(
                format!("Failed to lock cache: {}", e)
            ))
        }
    }).await {
        Ok(Ok(())) => {
            println!("✓ Manual refresh completed successfully");
            HttpResponse::Ok().json(ApiResponse::success("Data refreshed successfully"))
        }
        Ok(Err(e)) => {
            eprintln!("⚠️  Manual refresh failed: {}", e);
            HttpResponse::InternalServerError()
                .json(ApiResponse::<String>::error(
                    format!("Refresh failed: {}", e)
                ))
        }
        Err(e) => {
            eprintln!("❌ Manual refresh task panicked: {}", e);
            HttpResponse::InternalServerError()
                .json(ApiResponse::<String>::error(
                    "Refresh task panicked".to_string()
                ))
        }
    }
}

// ============================================================================
// Background Task
// ============================================================================

async fn data_refresh_task(state: Arc<Mutex<CachedNetworkData>>) {
    let mut interval = time::interval(Duration::from_secs(30));

    loop {
        interval.tick().await;

        println!("\n🔄 Auto-refreshing network data...");

        let state_clone = state.clone();
        match tokio::task::spawn_blocking(move || {
            match state_clone.lock() {
                Ok(mut cache) => NVTModels::smart_refresh(&mut cache),
                Err(e) => Err(tbm_api_models::NVTError::NetworkError(
                    format!("Failed to lock cache: {}", e)
                ))
            }
        }).await {
            Ok(Ok(())) => {
                println!("✓ Auto-refresh completed successfully at {}",
                         NVTModels::format_timestamp_full(NVTModels::get_current_timestamp()));
            }
            Ok(Err(e)) => {
                eprintln!("⚠️  Auto-refresh failed: {}", e);
            }
            Err(e) => {
                eprintln!("❌ Auto-refresh task panicked: {}", e);
            }
        }
    }
}

// ============================================================================
// Server Setup
// ============================================================================

async fn run_server(cache: CachedNetworkData) -> std::io::Result<()> {
    let app_state = AppState {
        cache: Arc::new(Mutex::new(cache)),
    };

    // Start background refresh task
    let refresh_cache = app_state.cache.clone();
    tokio::spawn(async move {
        data_refresh_task(refresh_cache).await;
    });

    println!("\n╔════════════════════════════════════════════════════════════╗");
    println!("║   🚀 TBM + TransGironde Transit Server (Embedded UI)      ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");
    println!("🌐 Server running on: http://0.0.0.0:8080");
    println!("📱 Web UI available at: http://localhost:8080");
    println!("📡 API available at: http://localhost:8080/api/tbm");
    println!("🔄 Auto-refresh: Every 30 seconds\n");

    println!("📍 Available Routes:");
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Frontend:                                                   │");
    println!("│   GET  /                           - Web UI (embedded)      │");
    println!("│   GET  /tbm-transit.js             - JavaScript (embedded)  │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!("│ API - Network Data:                                         │");
    println!("│   GET  /api/tbm/network            - Full network data      │");
    println!("│   GET  /api/tbm/stops              - All stops              │");
    println!("│   GET  /api/tbm/lines              - All lines              │");
    println!("│   GET  /api/tbm/vehicles           - Real-time vehicles     │");
    println!("│   GET  /api/tbm/alerts             - Active alerts          │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!("│ API - Specific Resources:                                   │");
    println!("│   GET  /api/tbm/stop/:id           - Stop by ID             │");
    println!("│   GET  /api/tbm/line/:code         - Line by code           │");
    println!("│   GET  /api/tbm/operator/:name     - Lines by operator      │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!("│ API - Meta & Control:                                       │");
    println!("│   GET  /api/tbm/operators          - List all operators     │");
    println!("│   GET  /api/tbm/stats              - Cache statistics       │");
    println!("│   POST /api/tbm/refresh            - Force refresh data     │");
    println!("│   GET  /health                     - Health check           │");
    println!("└─────────────────────────────────────────────────────────────┘\n");

    println!("💡 Quick Start:");
    println!("   1. Open your browser to: http://localhost:8080");
    println!("   2. The map will load automatically!");
    println!("   3. API available at: http://localhost:8080/api/tbm/*\n");

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            // Frontend routes
            .route("/", web::get().to(serve_index))
            .route("/tbm-transit.js", web::get().to(serve_js))
            // Health check
            .route("/health", web::get().to(health_check))
            // API routes
            .service(
                web::scope("/api/tbm")
                    .route("/network", web::get().to(get_network_data))
                    .route("/stops", web::get().to(get_stops))
                    .route("/lines", web::get().to(get_lines))
                    .route("/vehicles", web::get().to(get_vehicles))
                    .route("/alerts", web::get().to(get_alerts))
                    .route("/stop/{id}", web::get().to(get_stop_by_id))
                    .route("/line/{code}", web::get().to(get_line_by_code))
                    .route("/operator/{name}", web::get().to(get_lines_by_operator))
                    .route("/operators", web::get().to(get_operators))
                    .route("/stats", web::get().to(get_stats))
                    .route("/refresh", web::post().to(force_refresh))
            )
    })
        .bind(("0.0.0.0", 8080))?
        .run()
        .await
}

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() -> std::io::Result<()> {
    println!("\n╔════════════════════════════════════════════════════════════╗");
    println!("║                                                            ║");
    println!("║    🚀 TBM + TransGironde Transit Server                    ║");
    println!("║       with Embedded Web UI                                ║");
    println!("║                                                            ║");
    println!("║    Version: 1.1.0                                          ║");
    println!("║    User: Cyclolysisss                                      ║");
    println!("║    Date: 2025-11-18 09:49:20                               ║");
    println!("║                                                            ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");

    println!("📡 Initializing network data cache...");
    println!("   This includes both TBM and TransGironde data...\n");

    let cache = match NVTModels::initialize_cache() {
        Ok(cache) => {
            println!("\n╔════════════════════════════════════════════════════════════╗");
            println!("║  ✅ Cache Initialized Successfully!                        ║");
            println!("╚════════════════════════════════════════════════════════════╝");
            cache
        }
        Err(e) => {
            eprintln!("\n╔════════════════════════════════════════════════════════════╗");
            eprintln!("║  ❌ INITIALIZATION FAILED                                  ║");
            eprintln!("╚════════════════════════════════════════════════════════════╝");
            eprintln!("\n❌ Failed to initialize cache: {}", e);
            eprintln!("Server cannot start without initial data.");
            eprintln!("\n💡 Troubleshooting:");
            eprintln!("   1. Check your internet connection");
            eprintln!("   2. Verify API endpoints are accessible");
            eprintln!("   3. Check firewall settings");
            eprintln!("   4. Review error message above for specific issues\n");
            std::process::exit(1);
        }
    };

    actix_web::rt::System::new().block_on(run_server(cache))
}