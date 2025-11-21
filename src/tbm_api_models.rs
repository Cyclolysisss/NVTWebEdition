// API models and data fetching for TBM (Transports Bordeaux Métropole) and TransGironde
// TBM Official website: https://www.infotbm.com/
// TransGironde Official website: https://www.transgironde.fr/
//
// TBM API Endpoints:
// - Stop Discovery SIRI-Lite: https://bdx.mecatran.com/utw/ws/siri/2.0/bordeaux/stoppoints-discovery.json
// - Lines Discovery SIRI-Lite: https://bdx.mecatran.com/utw/ws/siri/2.0/bordeaux/lines-discovery.json
// - GTFS-RT Vehicles: https://bdx.mecatran.com/utw/ws/gtfsfeed/vehicles/bordeaux
// - GTFS-RT Alerts: https://bdx.mecatran.com/utw/ws/gtfsfeed/alerts/bordeaux
// - GTFS-RT Trip Updates: https://bdx.mecatran.com/utw/ws/gtfsfeed/realtime/bordeaux
//
// TransGironde Data:
// - GTFS Static: https://www.pigma.org/public/opendata/nouvelle_aquitaine_mobilites/publication/gironde-aggregated-gtfs.zip

use reqwest::blocking;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use gtfs_rt::FeedMessage;
use prost::Message;
use chrono::{TimeZone, Utc};
use chrono_tz::Europe::Paris;
use std::io::Read;
use std::io::Cursor;
use zip::ZipArchive;
use std::time::{SystemTime, UNIX_EPOCH};
use std::path::PathBuf;
use std::fs;

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertInfo {
    pub id: String,
    pub text: String,
    pub description: String,
    pub url: Option<String>,
    pub route_ids: Vec<String>,
    pub stop_ids: Vec<String>,
    pub active_period_start: Option<i64>,
    pub active_period_end: Option<i64>,
    pub severity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealTimeInfo {
    pub vehicle_id: String,
    pub trip_id: String,
    pub route_id: Option<String>,
    pub direction_id: Option<u32>,
    pub destination: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub stop_id: Option<String>,
    pub timestamp: Option<i64>,
    pub delay: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stop {
    pub stop_id: String,
    pub stop_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub lines: Vec<String>,
    pub alerts: Vec<AlertInfo>,
    pub real_time: Vec<RealTimeInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShapePoint {
    pub latitude: f64,
    pub longitude: f64,
    pub sequence: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Line {
    pub line_ref: String,
    pub line_name: String,
    pub line_code: String,
    pub route_id: String,
    pub destinations: Vec<(String, String)>,
    pub alerts: Vec<AlertInfo>,
    pub real_time: Vec<RealTimeInfo>,
    pub color: String,
    pub shape_ids: Vec<String>,
    pub operator: String, // "TBM" or "TransGironde"
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkData {
    pub stops: Vec<Stop>,
    pub lines: Vec<Line>,
    pub shapes: HashMap<String, Vec<ShapePoint>>,
}

// ============================================================================
// GTFS Cache Structure (15-day persistence for TBM, 30-day for TransGironde)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GTFSCache {
    pub routes: HashMap<String, String>,
    pub stops: Vec<(String, String, f64, f64)>,
    pub shapes: HashMap<String, Vec<ShapePoint>>,
    pub route_to_shapes: HashMap<String, Vec<String>>,
    pub cached_at: u64,
    pub source: String, // "TBM" or "TransGironde"
}

impl GTFSCache {
    pub fn is_expired(&self, max_age_days: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let age_days = (now.saturating_sub(self.cached_at)) / 86400;
        age_days >= max_age_days
    }

    pub fn cache_path(source: &str) -> PathBuf {
        let mut path = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("tbm_nvt");
        fs::create_dir_all(&path).ok();
        path.push(format!("{}_gtfs_cache.json", source.to_lowercase()));
        path
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::cache_path(&self.source);
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| NVTError::FileError(format!("Failed to serialize cache: {}", e)))?;

        fs::write(&path, json)
            .map_err(|e| NVTError::FileError(format!("Failed to write cache: {}", e)))?;

        println!("✓ {} GTFS cache saved to: {:?}", self.source, path);
        Ok(())
    }

    pub fn load(source: &str, max_age_days: u64) -> Option<Self> {
        let path = Self::cache_path(source);

        if !path.exists() {
            println!("ℹ️  No {} GTFS cache found, will download fresh data", source);
            return None;
        }

        match fs::read_to_string(&path) {
            Ok(contents) => {
                match serde_json::from_str::<GTFSCache>(&contents) {
                    Ok(cache) => {
                        if cache.is_expired(max_age_days) {
                            println!("⚠️  {} GTFS cache expired (>{} days old), refreshing...", source, max_age_days);
                            None
                        } else {
                            let age_days = (SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs().saturating_sub(cache.cached_at)) / 86400;
                            println!("✓ {} GTFS cache loaded ({} days old)", source, age_days);
                            println!("  • {} routes with colors", cache.routes.len());
                            println!("  • {} stops cached", cache.stops.len());
                            println!("  • {} shapes cached", cache.shapes.len());
                            Some(cache)
                        }
                    }
                    Err(e) => {
                        println!("⚠️  Failed to parse {} cache ({}), will refresh", source, e);
                        None
                    }
                }
            }
            Err(e) => {
                println!("⚠️  Failed to read {} cache file ({}), will refresh", source, e);
                None
            }
        }
    }
}

// ============================================================================
// Cache Structure for efficient refresh
// ============================================================================

#[derive(Debug, Clone)]
pub struct CachedNetworkData {
    // TBM Data
    pub tbm_stops_metadata: Vec<(String, String, f64, f64, Vec<String>)>,
    pub tbm_lines_metadata: Vec<(String, String, String, Vec<(String, String)>)>,
    pub tbm_gtfs_cache: GTFSCache,

    // TransGironde Data
    pub transgironde_stops: Vec<Stop>,
    pub transgironde_lines: Vec<Line>,
    pub transgironde_gtfs_cache: GTFSCache,

    // SNCF Data
    pub sncf_stops: Vec<Stop>,
    pub sncf_lines: Vec<Line>,
    pub sncf_gtfs_cache: GTFSCache,

    pub last_static_update: u64,
    pub alerts: Vec<AlertInfo>,
    pub real_time: Vec<RealTimeInfo>,
    pub trip_updates: Vec<gtfs_rt::TripUpdate>,
    pub last_dynamic_update: u64,
}

impl CachedNetworkData {
    pub fn needs_static_refresh(&self, max_age_seconds: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.last_static_update) > max_age_seconds
    }

    pub fn to_network_data(&self) -> NetworkData {
        let mut all_stops = NVTModels::build_stops(
            self.tbm_stops_metadata.clone(),
            self.alerts.clone(),
            self.real_time.clone(),
            self.trip_updates.clone(),
            &self.tbm_lines_metadata,
        );

        // Add TransGironde stops
        all_stops.extend(self.transgironde_stops.clone());

        // Add SNCF stops
        all_stops.extend(self.sncf_stops.clone());

        let mut all_lines = NVTModels::build_lines(
            self.tbm_lines_metadata.clone(),
            self.alerts.clone(),
            self.real_time.clone(),
            &self.tbm_gtfs_cache,
        );

        // Add TransGironde lines
        all_lines.extend(self.transgironde_lines.clone());

        // Add SNCF lines
        all_lines.extend(self.sncf_lines.clone());

        // Combine shapes
        let mut all_shapes = self.tbm_gtfs_cache.shapes.clone();
        all_shapes.extend(self.transgironde_gtfs_cache.shapes.clone());
        all_shapes.extend(self.sncf_gtfs_cache.shapes.clone());

        NetworkData {
            stops: all_stops,
            lines: all_lines,
            shapes: all_shapes,
        }
    }
}

// ============================================================================
// Error Handling
// ============================================================================

#[derive(Debug)]
pub enum NVTError {
    NetworkError(String),
    ParseError(String),
    FileError(String),
}

impl std::fmt::Display for NVTError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NVTError::NetworkError(e) => write!(f, "Network error: {}", e),
            NVTError::ParseError(e) => write!(f, "Parse error: {}", e),
            NVTError::FileError(e) => write!(f, "File error: {}", e),
        }
    }
}

impl std::error::Error for NVTError {}

pub type Result<T> = std::result::Result<T, NVTError>;

// ============================================================================
// Main Implementation
// ============================================================================

pub struct NVTModels;

impl NVTModels {
    const API_KEY: &'static str = "opendata-bordeaux-metropole-flux-gtfs-rt";
    const BASE_URL: &'static str = "https://bdx.mecatran.com/utw/ws";
    const TRANSGIRONDE_GTFS_URL: &'static str = "https://www.pigma.org/public/opendata/nouvelle_aquitaine_mobilites/publication/gironde-aggregated-gtfs.zip";
    const SNCF_GTFS_URL: &'static str = "https://eu.ftp.opendatasoft.com/sncf/plandata/Export_OpenData_SNCF_GTFS_NewTripId.zip";
    const SNCF_GTFS_RT_TRIP_UPDATES_URL: &'static str = "https://proxy.transport.data.gouv.fr/resource/sncf-gtfs-rt-trip-updates";
    const SNCF_GTFS_RT_SERVICE_ALERTS_URL: &'static str = "https://proxy.transport.data.gouv.fr/resource/sncf-gtfs-rt-service-alerts";
    const STATIC_DATA_MAX_AGE: u64 = 3600;
    const REQUEST_TIMEOUT_SECS: u64 = 30;

    pub fn initialize_cache() -> Result<CachedNetworkData> {
        println!("🔄 Initializing network data cache...");
        println!("   This may take a moment...");

        // Load TBM data
        println!("\n📍 Loading TBM data...");
        let tbm_stops = Self::fetch_stops().map_err(|e| {
            NVTError::NetworkError(format!("Failed to fetch TBM stops: {}", e))
        })?;
        println!("   ✓ Loaded {} TBM stops", tbm_stops.len());

        let tbm_lines = Self::fetch_lines().map_err(|e| {
            NVTError::NetworkError(format!("Failed to fetch TBM lines: {}", e))
        })?;
        println!("   ✓ Loaded {} TBM lines", tbm_lines.len());

        let tbm_gtfs_cache = Self::load_gtfs_data("TBM", 15).unwrap_or_else(|e| {
            println!("   ⚠️  Warning: Could not load TBM GTFS data ({})", e);
            println!("   Continuing with default colors...");
            GTFSCache {
                routes: HashMap::new(),
                stops: Vec::new(),
                shapes: HashMap::new(),
                route_to_shapes: HashMap::new(),
                cached_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                source: "TBM".to_string(),
            }
        });
        println!("   ✓ Loaded {} TBM line colors", tbm_gtfs_cache.routes.len());

        // Load TransGironde data
        println!("\n🚌 Loading TransGironde data...");
        let (transgironde_stops, transgironde_lines, transgironde_gtfs_cache) =
            Self::load_transgironde_data().unwrap_or_else(|e| {
                println!("   ⚠️  Warning: Could not load TransGironde data ({})", e);
                println!("   Continuing without TransGironde...");
                (Vec::new(), Vec::new(), GTFSCache {
                    routes: HashMap::new(),
                    stops: Vec::new(),
                    shapes: HashMap::new(),
                    route_to_shapes: HashMap::new(),
                    cached_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    source: "TransGironde".to_string(),
                })
            });
        println!("   ✓ Loaded {} TransGironde stops", transgironde_stops.len());
        println!("   ✓ Loaded {} TransGironde lines", transgironde_lines.len());
        println!("   ✓ Loaded {} TransGironde shapes", transgironde_gtfs_cache.shapes.len());

        // Load SNCF data
        println!("\n🚄 Loading SNCF data...");
        let (sncf_stops, sncf_lines, sncf_gtfs_cache) =
            Self::load_sncf_data().unwrap_or_else(|e| {
                println!("   ⚠️  Warning: Could not load SNCF data ({})", e);
                println!("   Continuing without SNCF...");
                (Vec::new(), Vec::new(), GTFSCache {
                    routes: HashMap::new(),
                    stops: Vec::new(),
                    shapes: HashMap::new(),
                    route_to_shapes: HashMap::new(),
                    cached_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    source: "SNCF".to_string(),
                })
            });
        println!("   ✓ Loaded {} SNCF stops", sncf_stops.len());
        println!("   ✓ Loaded {} SNCF lines", sncf_lines.len());
        println!("   ✓ Loaded {} SNCF shapes", sncf_gtfs_cache.shapes.len());

        // Load real-time data
        println!("\n📡 Loading real-time data...");
        let alerts = Self::fetch_alerts().unwrap_or_else(|e| {
            println!("   ⚠️  Warning: Could not fetch alerts ({})", e);
            Vec::new()
        });
        println!("   ✓ Loaded {} alerts", alerts.len());

        let real_time = Self::fetch_vehicle_positions().unwrap_or_else(|e| {
            println!("   ⚠️  Warning: Could not fetch vehicle positions ({})", e);
            Vec::new()
        });
        println!("   ✓ Loaded {} vehicle positions", real_time.len());

        let trip_updates = Self::fetch_trip_updates().unwrap_or_else(|e| {
            println!("   ⚠️  Warning: Could not fetch trip updates ({})", e);
            Vec::new()
        });
        println!("   ✓ Loaded {} trip updates", trip_updates.len());

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        println!("\n✓ Cache initialized successfully!");
        println!("  • TBM: {} stops, {} lines", tbm_stops.len(), tbm_lines.len());
        println!("  • TransGironde: {} stops, {} lines", transgironde_stops.len(), transgironde_lines.len());
        println!("  • SNCF: {} stops, {} lines", sncf_stops.len(), sncf_lines.len());
        println!("  • {} vehicles tracked, {} alerts", real_time.len(), alerts.len());

        Ok(CachedNetworkData {
            tbm_stops_metadata: tbm_stops,
            tbm_lines_metadata: tbm_lines,
            tbm_gtfs_cache,
            transgironde_stops,
            transgironde_lines,
            transgironde_gtfs_cache,
            sncf_stops,
            sncf_lines,
            sncf_gtfs_cache,
            last_static_update: now,
            alerts,
            real_time,
            trip_updates,
            last_dynamic_update: now,
        })
    }

    pub fn refresh_dynamic_data(cache: &mut CachedNetworkData) -> Result<()> {
        cache.alerts = Self::fetch_alerts().unwrap_or_else(|e| {
            eprintln!("⚠️  Warning: Could not fetch alerts ({})", e);
            cache.alerts.clone()
        });

        cache.real_time = Self::fetch_vehicle_positions().unwrap_or_else(|e| {
            eprintln!("⚠️  Warning: Could not fetch vehicle positions ({})", e);
            cache.real_time.clone()
        });

        cache.trip_updates = Self::fetch_trip_updates().unwrap_or_else(|e| {
            eprintln!("⚠️  Warning: Could not fetch trip updates ({})", e);
            cache.trip_updates.clone()
        });

        cache.last_dynamic_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(())
    }

    pub fn refresh_static_data(cache: &mut CachedNetworkData) -> Result<()> {
        println!("🔄 Refreshing static network data...");

        cache.tbm_stops_metadata = Self::fetch_stops()?;
        cache.tbm_lines_metadata = Self::fetch_lines()?;
        cache.tbm_gtfs_cache = Self::load_gtfs_data("TBM", 15)
            .unwrap_or(cache.tbm_gtfs_cache.clone());

        let (transgironde_stops, transgironde_lines, transgironde_gtfs_cache) =
            Self::load_transgironde_data()
                .unwrap_or((cache.transgironde_stops.clone(),
                            cache.transgironde_lines.clone(),
                            cache.transgironde_gtfs_cache.clone()));

        cache.transgironde_stops = transgironde_stops;
        cache.transgironde_lines = transgironde_lines;
        cache.transgironde_gtfs_cache = transgironde_gtfs_cache;

        let (sncf_stops, sncf_lines, sncf_gtfs_cache) =
            Self::load_sncf_data()
                .unwrap_or((cache.sncf_stops.clone(),
                            cache.sncf_lines.clone(),
                            cache.sncf_gtfs_cache.clone()));

        cache.sncf_stops = sncf_stops;
        cache.sncf_lines = sncf_lines;
        cache.sncf_gtfs_cache = sncf_gtfs_cache;

        cache.last_static_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        println!("✓ Static data refreshed!");

        Ok(())
    }

    pub fn smart_refresh(cache: &mut CachedNetworkData) -> Result<()> {
        Self::refresh_dynamic_data(cache)?;

        if cache.needs_static_refresh(Self::STATIC_DATA_MAX_AGE) {
            Self::refresh_static_data(cache)?;
        }

        Ok(())
    }

    // ============================================================================
    // TransGironde GTFS Loading
    // ============================================================================

    fn load_transgironde_data() -> Result<(Vec<Stop>, Vec<Line>, GTFSCache)> {
        if let Some(cache) = GTFSCache::load("TransGironde", 30) {
            return Self::parse_transgironde_from_cache(cache);
        }

        println!("📥 Downloading TransGironde GTFS data...");

        let client = blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(Self::REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| NVTError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let response = client.get(Self::TRANSGIRONDE_GTFS_URL)
            .send()
            .map_err(|e| NVTError::NetworkError(format!("Failed to download TransGironde GTFS: {}", e)))?;

        if !response.status().is_success() {
            return Err(NVTError::NetworkError(format!("Download failed with status: {}", response.status())));
        }

        let zip_bytes = response.bytes()
            .map_err(|e| NVTError::NetworkError(format!("Failed to read GTFS zip: {}", e)))?;

        println!("✓ Downloaded {} KB, extracting...", zip_bytes.len() / 1024);

        let cursor = Cursor::new(zip_bytes);
        let mut archive = ZipArchive::new(cursor)
            .map_err(|e| NVTError::ParseError(format!("Failed to open GTFS zip: {}", e)))?;

        // Parse routes.txt
        let routes = Self::parse_transgironde_routes(&mut archive)?;
        println!("   ✓ Parsed {} TransGironde routes", routes.len());

        // Parse stops.txt
        let stops_data = Self::parse_transgironde_stops(&mut archive)?;
        println!("   ✓ Parsed {} TransGironde stops", stops_data.len());

        // Parse shapes.txt
        let shapes = Self::parse_transgironde_shapes(&mut archive)?;
        println!("   ✓ Parsed {} TransGironde shapes", shapes.len());

        // Parse trips.txt to map routes to shapes
        let route_to_shapes = Self::parse_transgironde_trips(&mut archive)?;
        println!("   ✓ Mapped {} routes to shapes", route_to_shapes.len());

        let gtfs_cache = GTFSCache {
            routes,
            stops: stops_data.clone(),
            shapes: shapes.clone(),
            route_to_shapes: route_to_shapes.clone(),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            source: "TransGironde".to_string(),
        };

        if let Err(e) = gtfs_cache.save() {
            eprintln!("⚠️  Warning: Could not save TransGironde cache: {}", e);
        }

        Self::parse_transgironde_from_cache(gtfs_cache)
    }

    fn parse_transgironde_routes(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<HashMap<String, String>> {
        let mut routes_file = archive.by_name("routes.txt")
            .map_err(|e| NVTError::FileError(format!("routes.txt not found: {}", e)))?;

        let mut routes_contents = String::new();
        routes_file.read_to_string(&mut routes_contents)
            .map_err(|e| NVTError::FileError(format!("Failed to read routes.txt: {}", e)))?;

        drop(routes_file);

        let mut color_map = HashMap::new();
        let mut rdr = csv::Reader::from_reader(routes_contents.as_bytes());

        for result in rdr.records() {
            if let Ok(record) = result {
                // route_id, route_short_name, route_long_name, route_color
                if let (Some(route_id), Some(route_color)) = (record.get(0), record.get(7)) {
                    if !route_color.is_empty() && route_color.len() == 6 {
                        color_map.insert(route_id.to_string(), route_color.to_string());
                    }
                }
            }
        }

        Ok(color_map)
    }

    fn parse_transgironde_stops(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<Vec<(String, String, f64, f64)>> {
        let mut stops_file = archive.by_name("stops.txt")
            .map_err(|e| NVTError::FileError(format!("stops.txt not found: {}", e)))?;

        let mut stops_contents = String::new();
        stops_file.read_to_string(&mut stops_contents)
            .map_err(|e| NVTError::FileError(format!("Failed to read stops.txt: {}", e)))?;

        drop(stops_file);

        let mut stops_data = Vec::new();
        let mut rdr = csv::Reader::from_reader(stops_contents.as_bytes());

        for result in rdr.records() {
            if let Ok(record) = result {
                // Only process stops (location_type = 1 means station, we want individual stops)
                if let (Some(stop_id), Some(stop_name), Some(lat_str), Some(lon_str), Some(location_type)) =
                    (record.get(0), record.get(2), record.get(5), record.get(6), record.get(9)) {

                    // Skip parent stations (location_type = 1)
                    if location_type == "1" {
                        continue;
                    }

                    if let (Ok(lat), Ok(lon)) = (lat_str.parse::<f64>(), lon_str.parse::<f64>()) {
                        if lat != 0.0 && lon != 0.0 {
                            stops_data.push((
                                stop_id.to_string(),
                                stop_name.to_string(),
                                lat,
                                lon,
                            ));
                        }
                    }
                }
            }
        }

        Ok(stops_data)
    }

    fn parse_transgironde_shapes(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<HashMap<String, Vec<ShapePoint>>> {
        let mut shapes_map: HashMap<String, Vec<ShapePoint>> = HashMap::new();

        if let Ok(mut shapes_file) = archive.by_name("shapes.txt") {
            let mut shapes_contents = String::new();
            shapes_file.read_to_string(&mut shapes_contents).ok();
            drop(shapes_file);

            let mut shapes_rdr = csv::Reader::from_reader(shapes_contents.as_bytes());

            for result in shapes_rdr.records() {
                if let Ok(record) = result {
                    if let (Some(shape_id), Some(lat_str), Some(lon_str), Some(seq_str)) =
                        (record.get(0), record.get(1), record.get(2), record.get(3)) {
                        if let (Ok(lat), Ok(lon), Ok(seq)) =
                            (lat_str.parse::<f64>(), lon_str.parse::<f64>(), seq_str.parse::<u32>()) {

                            shapes_map.entry(shape_id.to_string())
                                .or_insert_with(Vec::new)
                                .push(ShapePoint {
                                    latitude: lat,
                                    longitude: lon,
                                    sequence: seq,
                                });
                        }
                    }
                }
            }

            for points in shapes_map.values_mut() {
                points.sort_by_key(|p| p.sequence);
            }
        }

        Ok(shapes_map)
    }

    fn parse_transgironde_trips(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<HashMap<String, Vec<String>>> {
        let mut route_to_shapes: HashMap<String, Vec<String>> = HashMap::new();

        if let Ok(mut trips_file) = archive.by_name("trips.txt") {
            let mut trips_contents = String::new();
            trips_file.read_to_string(&mut trips_contents).ok();
            drop(trips_file);

            let mut trips_rdr = csv::Reader::from_reader(trips_contents.as_bytes());

            for result in trips_rdr.records() {
                if let Ok(record) = result {
                    // route_id is field 0, shape_id is field 7
                    if let (Some(route_id), Some(shape_id)) = (record.get(0), record.get(7)) {
                        if !shape_id.is_empty() {
                            route_to_shapes.entry(route_id.to_string())
                                .or_insert_with(Vec::new)
                                .push(shape_id.to_string());
                        }
                    }
                }
            }

            for shape_ids in route_to_shapes.values_mut() {
                shape_ids.sort();
                shape_ids.dedup();
            }
        }

        Ok(route_to_shapes)
    }

    fn parse_transgironde_from_cache(cache: GTFSCache) -> Result<(Vec<Stop>, Vec<Line>, GTFSCache)> {
        let mut stops = Vec::new();
        let mut stops_by_parent: HashMap<String, Vec<String>> = HashMap::new();

        // Create stops
        for (stop_id, stop_name, lat, lon) in &cache.stops {
            stops.push(Stop {
                stop_id: stop_id.clone(),
                stop_name: stop_name.clone(),
                latitude: *lat,
                longitude: *lon,
                lines: Vec::new(), // Will be populated when we process routes
                alerts: Vec::new(),
                real_time: Vec::new(),
            });
        }

        // Create lines from routes
        let mut lines = Vec::new();
        for (route_id, color) in &cache.routes {
            // Extract route short name from route_id
            // Format: "GIRONDE:Line:2743" -> "414" (from routes.txt)
            let line_code = route_id.split(':').last().unwrap_or(route_id);

            let shape_ids = cache.route_to_shapes.get(route_id)
                .cloned()
                .unwrap_or_default();

            lines.push(Line {
                line_ref: route_id.clone(),
                line_name: format!("TransGironde {}", line_code),
                line_code: line_code.to_string(),
                route_id: route_id.clone(),
                destinations: Vec::new(),
                alerts: Vec::new(),
                real_time: Vec::new(),
                color: color.clone(),
                shape_ids,
                operator: "TransGironde".to_string(),
            });
        }

        Ok((stops, lines, cache))
    }

    // ============================================================================
    // SNCF GTFS Loading
    // ============================================================================

    fn load_sncf_data() -> Result<(Vec<Stop>, Vec<Line>, GTFSCache)> {
        if let Some(cache) = GTFSCache::load("SNCF", 30) {
            return Self::parse_sncf_from_cache(cache);
        }

        println!("📥 Downloading SNCF GTFS data...");

        let client = blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(Self::REQUEST_TIMEOUT_SECS * 3)) // Longer timeout for large file
            .build()
            .map_err(|e| NVTError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let response = client.get(Self::SNCF_GTFS_URL)
            .send()
            .map_err(|e| NVTError::NetworkError(format!("Failed to download SNCF GTFS: {}", e)))?;

        if !response.status().is_success() {
            return Err(NVTError::NetworkError(format!("Download failed with status: {}", response.status())));
        }

        let zip_bytes = response.bytes()
            .map_err(|e| NVTError::NetworkError(format!("Failed to read GTFS zip: {}", e)))?;

        println!("✓ Downloaded {} MB, extracting...", zip_bytes.len() / 1024 / 1024);

        let cursor = Cursor::new(zip_bytes);
        let mut archive = ZipArchive::new(cursor)
            .map_err(|e| NVTError::ParseError(format!("Failed to open GTFS zip: {}", e)))?;

        // Parse routes.txt
        let routes = Self::parse_sncf_routes(&mut archive)?;
        println!("   ✓ Parsed {} SNCF routes", routes.len());

        // Parse stops.txt
        let stops_data = Self::parse_sncf_stops(&mut archive)?;
        println!("   ✓ Parsed {} SNCF stops", stops_data.len());

        // Parse shapes.txt
        let shapes = Self::parse_sncf_shapes(&mut archive)?;
        println!("   ✓ Parsed {} SNCF shapes", shapes.len());

        // Parse trips.txt to map routes to shapes
        let route_to_shapes = Self::parse_sncf_trips(&mut archive)?;
        println!("   ✓ Mapped {} routes to shapes", route_to_shapes.len());

        let gtfs_cache = GTFSCache {
            routes,
            stops: stops_data.clone(),
            shapes: shapes.clone(),
            route_to_shapes: route_to_shapes.clone(),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            source: "SNCF".to_string(),
        };

        if let Err(e) = gtfs_cache.save() {
            eprintln!("⚠️  Warning: Could not save SNCF cache: {}", e);
        }

        Self::parse_sncf_from_cache(gtfs_cache)
    }

    fn parse_sncf_routes(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<HashMap<String, String>> {
        let mut routes_file = archive.by_name("routes.txt")
            .map_err(|e| NVTError::FileError(format!("routes.txt not found: {}", e)))?;

        let mut routes_contents = String::new();
        routes_file.read_to_string(&mut routes_contents)
            .map_err(|e| NVTError::FileError(format!("Failed to read routes.txt: {}", e)))?;

        drop(routes_file);

        let mut color_map = HashMap::new();
        let mut rdr = csv::Reader::from_reader(routes_contents.as_bytes());

        for result in rdr.records() {
            if let Ok(record) = result {
                // route_id, route_short_name, route_long_name, ..., route_color
                if let (Some(route_id), Some(route_color)) = (record.get(0), record.get(7)) {
                    if !route_color.is_empty() && route_color.len() == 6 {
                        color_map.insert(route_id.to_string(), route_color.to_string());
                    }
                }
            }
        }

        Ok(color_map)
    }

    fn extract_sncf_stop_id(full_id: &str) -> Option<String> {
        // SNCF stop_id format: "StopPoint:OCETGV INOUI-87192039" -> "87192039"
        // or "StopPoint:OCETrain TER-71793150" -> "71793150"
        if let Some(dash_pos) = full_id.rfind('-') {
            Some(full_id[dash_pos + 1..].to_string())
        } else {
            Some(full_id.to_string())
        }
    }

    fn parse_sncf_stops(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<Vec<(String, String, f64, f64)>> {
        let mut stops_file = archive.by_name("stops.txt")
            .map_err(|e| NVTError::FileError(format!("stops.txt not found: {}", e)))?;

        let mut stops_contents = String::new();
        stops_file.read_to_string(&mut stops_contents)
            .map_err(|e| NVTError::FileError(format!("Failed to read stops.txt: {}", e)))?;

        drop(stops_file);

        let mut stops_data = Vec::new();
        let mut rdr = csv::Reader::from_reader(stops_contents.as_bytes());

        for result in rdr.records() {
            if let Ok(record) = result {
                // stop_id, stop_code, stop_name, stop_desc, stop_lat, stop_lon, ..., location_type
                if let (Some(stop_id), Some(stop_name), Some(lat_str), Some(lon_str)) =
                    (record.get(0), record.get(2), record.get(4), record.get(5)) {

                    // Check location_type if available (0 = stop/platform, 1 = station)
                    let location_type = record.get(9).unwrap_or("0");
                    
                    // Skip parent stations (location_type = 1)
                    if location_type == "1" {
                        continue;
                    }

                    if let (Ok(lat), Ok(lon)) = (lat_str.parse::<f64>(), lon_str.parse::<f64>()) {
                        if lat != 0.0 && lon != 0.0 {
                            // Extract the simplified stop ID
                            if let Some(simplified_id) = Self::extract_sncf_stop_id(stop_id) {
                                stops_data.push((
                                    simplified_id,
                                    stop_name.to_string(),
                                    lat,
                                    lon,
                                ));
                            }
                        }
                    }
                }
            }
        }

        Ok(stops_data)
    }

    fn parse_sncf_shapes(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<HashMap<String, Vec<ShapePoint>>> {
        let mut shapes_map: HashMap<String, Vec<ShapePoint>> = HashMap::new();

        if let Ok(mut shapes_file) = archive.by_name("shapes.txt") {
            let mut shapes_contents = String::new();
            shapes_file.read_to_string(&mut shapes_contents).ok();
            drop(shapes_file);

            let mut shapes_rdr = csv::Reader::from_reader(shapes_contents.as_bytes());

            for result in shapes_rdr.records() {
                if let Ok(record) = result {
                    if let (Some(shape_id), Some(lat_str), Some(lon_str), Some(seq_str)) =
                        (record.get(0), record.get(1), record.get(2), record.get(3)) {
                        if let (Ok(lat), Ok(lon), Ok(seq)) =
                            (lat_str.parse::<f64>(), lon_str.parse::<f64>(), seq_str.parse::<u32>()) {

                            shapes_map.entry(shape_id.to_string())
                                .or_insert_with(Vec::new)
                                .push(ShapePoint {
                                    latitude: lat,
                                    longitude: lon,
                                    sequence: seq,
                                });
                        }
                    }
                }
            }

            for points in shapes_map.values_mut() {
                points.sort_by_key(|p| p.sequence);
            }
        }

        Ok(shapes_map)
    }

    fn parse_sncf_trips(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<HashMap<String, Vec<String>>> {
        let mut route_to_shapes: HashMap<String, Vec<String>> = HashMap::new();

        if let Ok(mut trips_file) = archive.by_name("trips.txt") {
            let mut trips_contents = String::new();
            trips_file.read_to_string(&mut trips_contents).ok();
            drop(trips_file);

            let mut trips_rdr = csv::Reader::from_reader(trips_contents.as_bytes());

            for result in trips_rdr.records() {
                if let Ok(record) = result {
                    // route_id is typically field 0, shape_id varies by GTFS spec
                    if let (Some(route_id), Some(shape_id)) = (record.get(0), record.get(7)) {
                        if !shape_id.is_empty() {
                            route_to_shapes.entry(route_id.to_string())
                                .or_insert_with(Vec::new)
                                .push(shape_id.to_string());
                        }
                    }
                }
            }

            for shape_ids in route_to_shapes.values_mut() {
                shape_ids.sort();
                shape_ids.dedup();
            }
        }

        Ok(route_to_shapes)
    }

    fn parse_sncf_from_cache(cache: GTFSCache) -> Result<(Vec<Stop>, Vec<Line>, GTFSCache)> {
        let mut stops = Vec::new();

        // Create stops
        for (stop_id, stop_name, lat, lon) in &cache.stops {
            stops.push(Stop {
                stop_id: stop_id.clone(),
                stop_name: stop_name.clone(),
                latitude: *lat,
                longitude: *lon,
                lines: Vec::new(),
                alerts: Vec::new(),
                real_time: Vec::new(),
            });
        }

        // Create lines from routes
        let mut lines = Vec::new();
        for (route_id, color) in &cache.routes {
            // Extract route short name from route_id for display
            let line_code = route_id.split(':').last().unwrap_or(route_id);

            let shape_ids = cache.route_to_shapes.get(route_id)
                .cloned()
                .unwrap_or_default();

            lines.push(Line {
                line_ref: route_id.clone(),
                line_name: format!("SNCF {}", line_code),
                line_code: line_code.to_string(),
                route_id: route_id.clone(),
                destinations: Vec::new(),
                alerts: Vec::new(),
                real_time: Vec::new(),
                color: color.clone(),
                shape_ids,
                operator: "SNCF".to_string(),
            });
        }

        Ok((stops, lines, cache))
    }

    // ============================================================================
    // TBM Data Fetching (existing methods)
    // ============================================================================

    fn fetch_stops() -> Result<Vec<(String, String, f64, f64, Vec<String>)>> {
        let url = format!(
            "{}/siri/2.0/bordeaux/stoppoints-discovery.json?AccountKey={}",
            Self::BASE_URL,
            Self::API_KEY
        );

        let client = blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(Self::REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| NVTError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let response = client.get(&url)
            .send()
            .map_err(|e| NVTError::NetworkError(format!("Failed to fetch stops: {}", e)))?;

        if !response.status().is_success() {
            return Err(NVTError::NetworkError(format!("API returned error: {}", response.status())));
        }

        let body = response.text()
            .map_err(|e| NVTError::NetworkError(format!("Failed to read response: {}", e)))?;

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| NVTError::ParseError(format!("Invalid JSON response: {}", e)))?;

        let stop_points = json["Siri"]["StopPointsDelivery"]["AnnotatedStopPointRef"]
            .as_array()
            .ok_or_else(|| NVTError::ParseError("Missing stop points data".to_string()))?;

        let stops: Vec<_> = stop_points
            .iter()
            .filter_map(|stop| {
                let full_id = stop["StopPointRef"]["value"].as_str()?;
                let stop_id = Self::extract_stop_id(full_id)?;
                let stop_name = stop["StopName"]["value"].as_str()?.to_string();
                let latitude = stop["Location"]["latitude"].as_f64()?;
                let longitude = stop["Location"]["longitude"].as_f64()?;
                let lines = stop["Lines"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|line| line["value"].as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                Some((stop_id, stop_name, latitude, longitude, lines))
            })
            .collect();

        if stops.is_empty() {
            return Err(NVTError::ParseError("No valid stops found".to_string()));
        }

        Ok(stops)
    }

    fn fetch_lines() -> Result<Vec<(String, String, String, Vec<(String, String)>)>> {
        let url = format!(
            "{}/siri/2.0/bordeaux/lines-discovery.json?AccountKey={}",
            Self::BASE_URL,
            Self::API_KEY
        );

        let client = blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(Self::REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| NVTError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let response = client.get(&url)
            .send()
            .map_err(|e| NVTError::NetworkError(format!("Failed to fetch lines: {}", e)))?;

        if !response.status().is_success() {
            return Err(NVTError::NetworkError(format!("API returned error: {}", response.status())));
        }

        let body = response.text()
            .map_err(|e| NVTError::NetworkError(format!("Failed to read response: {}", e)))?;

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| NVTError::ParseError(format!("Invalid JSON response: {}", e)))?;

        let line_refs = json["Siri"]["LinesDelivery"]["AnnotatedLineRef"]
            .as_array()
            .ok_or_else(|| NVTError::ParseError("Missing lines data".to_string()))?;

        let lines: Vec<_> = line_refs
            .iter()
            .filter_map(|line| {
                let line_ref = line["LineRef"]["value"].as_str()?.to_string();
                let line_name = line["LineName"][0]["value"].as_str()?.to_string();
                let line_code = line["LineCode"]["value"].as_str()?.to_string();
                let destinations = line["Destinations"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|dest| {
                                let direction = dest["DirectionRef"]["value"].as_str()?.to_string();
                                let place = dest["PlaceName"][0]["value"].as_str()?.to_string();
                                Some((direction, place))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                Some((line_ref, line_name, line_code, destinations))
            })
            .collect();

        if lines.is_empty() {
            return Err(NVTError::ParseError("No valid lines found".to_string()));
        }

        Ok(lines)
    }

    fn fetch_alerts() -> Result<Vec<AlertInfo>> {
        let url = format!(
            "{}/gtfsfeed/alerts/bordeaux?apiKey={}",
            Self::BASE_URL,
            Self::API_KEY
        );

        let client = blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(Self::REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| NVTError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let response = client.get(&url)
            .send()
            .map_err(|e| NVTError::NetworkError(format!("Failed to fetch alerts: {}", e)))?;

        let body = response.bytes()
            .map_err(|e| NVTError::NetworkError(format!("Failed to read alerts response: {}", e)))?;

        let feed = FeedMessage::decode(&*body)
            .map_err(|e| NVTError::ParseError(format!("Failed to decode alerts feed: {}", e)))?;

        let alerts = feed
            .entity
            .into_iter()
            .filter_map(|entity| {
                entity.alert.map(|alert| {
                    let header_text = alert
                        .header_text
                        .and_then(|h| h.translation.first().map(|t| t.text.clone()))
                        .unwrap_or_else(|| "No title".to_string());

                    let description_text = alert
                        .description_text
                        .and_then(|d| d.translation.first().map(|t| t.text.clone()))
                        .unwrap_or_else(|| "No description available".to_string());

                    let url = alert
                        .url
                        .and_then(|u| u.translation.first().map(|t| t.text.clone()));

                    let mut route_ids = Vec::new();
                    let mut stop_ids = Vec::new();

                    for informed_entity in alert.informed_entity {
                        if let Some(route_id) = informed_entity.route_id {
                            route_ids.push(route_id);
                        }
                        if let Some(stop_id) = informed_entity.stop_id {
                            stop_ids.push(stop_id);
                        }
                    }

                    let (start, end) = alert.active_period
                        .first()
                        .map(|period| {
                            (
                                period.start.map(|s| s as i64),
                                period.end.map(|e| e as i64)
                            )
                        })
                        .unwrap_or((None, None));

                    let severity = alert.severity_level.unwrap_or(0) as u32;

                    AlertInfo {
                        id: entity.id,
                        text: header_text,
                        description: description_text,
                        url,
                        route_ids,
                        stop_ids,
                        active_period_start: start,
                        active_period_end: end,
                        severity,
                    }
                })
            })
            .collect();

        Ok(alerts)
    }

    fn fetch_vehicle_positions() -> Result<Vec<RealTimeInfo>> {
        let url = format!(
            "{}/gtfsfeed/vehicles/bordeaux?apiKey={}",
            Self::BASE_URL,
            Self::API_KEY
        );

        let client = blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(Self::REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| NVTError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let response = client.get(&url)
            .send()
            .map_err(|e| NVTError::NetworkError(format!("Failed to fetch vehicle positions: {}", e)))?;

        let body = response.bytes()
            .map_err(|e| NVTError::NetworkError(format!("Failed to read vehicles response: {}", e)))?;

        let feed = FeedMessage::decode(&*body)
            .map_err(|e| NVTError::ParseError(format!("Failed to decode vehicles feed: {}", e)))?;

        let real_time: Vec<RealTimeInfo> = feed
            .entity
            .into_iter()
            .filter_map(|entity| {
                entity.vehicle.map(|vehicle| {
                    let vehicle_id = vehicle
                        .vehicle
                        .as_ref()
                        .and_then(|v| v.id.clone())
                        .unwrap_or_else(|| "Unknown".to_string());

                    let trip_id = vehicle
                        .trip
                        .as_ref()
                        .and_then(|t| t.trip_id.clone())
                        .unwrap_or_else(|| "Unknown".to_string());

                    let route_id = vehicle
                        .trip
                        .as_ref()
                        .and_then(|t| t.route_id.clone());

                    let direction_id = vehicle
                        .trip
                        .as_ref()
                        .and_then(|t| t.direction_id);

                    let destination = vehicle
                        .vehicle
                        .as_ref()
                        .and_then(|v| v.label.clone());

                    let (latitude, longitude) = vehicle
                        .position
                        .as_ref()
                        .map(|p| (p.latitude as f64, p.longitude as f64))
                        .unwrap_or((0.0, 0.0));

                    let stop_id = vehicle.stop_id.clone();
                    let timestamp = vehicle.timestamp.map(|ts| ts as i64);

                    RealTimeInfo {
                        vehicle_id,
                        trip_id,
                        route_id,
                        direction_id,
                        destination,
                        latitude,
                        longitude,
                        stop_id,
                        timestamp,
                        delay: None,
                    }
                })
            })
            .collect();

        Ok(real_time)
    }

    fn fetch_trip_updates() -> Result<Vec<gtfs_rt::TripUpdate>> {
        let url = format!(
            "{}/gtfsfeed/realtime/bordeaux?apiKey={}",
            Self::BASE_URL,
            Self::API_KEY
        );

        let client = blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(Self::REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| NVTError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let response = client.get(&url)
            .send()
            .map_err(|e| NVTError::NetworkError(format!("Failed to fetch trip updates: {}", e)))?;

        let body = response.bytes()
            .map_err(|e| NVTError::NetworkError(format!("Failed to read trip updates response: {}", e)))?;

        let feed = FeedMessage::decode(&*body)
            .map_err(|e| NVTError::ParseError(format!("Failed to decode trip updates feed: {}", e)))?;

        let updates = feed
            .entity
            .into_iter()
            .filter_map(|entity| entity.trip_update)
            .collect();

        Ok(updates)
    }

    fn download_and_read_gtfs() -> Result<GTFSCache> {
        if let Some(cache) = GTFSCache::load("TBM", 15) {
            return Ok(cache);
        }

        println!("📥 Downloading fresh TBM GTFS data...");
        let gtfs_url = "https://transport.data.gouv.fr/resources/83024/download";

        let client = blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| NVTError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let response = client.get(gtfs_url)
            .send()
            .map_err(|e| NVTError::NetworkError(format!("Failed to download GTFS: {}", e)))?;

        if !response.status().is_success() {
            return Err(NVTError::NetworkError(format!("GTFS download failed with status: {}", response.status())));
        }

        let zip_bytes = response.bytes()
            .map_err(|e| NVTError::NetworkError(format!("Failed to read GTFS zip: {}", e)))?;

        println!("✓ Downloaded {} KB, extracting...", zip_bytes.len() / 1024);

        let cursor = Cursor::new(zip_bytes);
        let mut archive = ZipArchive::new(cursor)
            .map_err(|e| NVTError::ParseError(format!("Failed to open GTFS zip archive: {}", e)))?;

        let mut routes_file = archive.by_name("routes.txt")
            .map_err(|e| NVTError::FileError(format!("routes.txt not found in GTFS archive: {}", e)))?;

        let mut routes_contents = String::new();
        routes_file.read_to_string(&mut routes_contents)
            .map_err(|e| NVTError::FileError(format!("Failed to read routes.txt: {}", e)))?;

        drop(routes_file);

        let mut color_map = HashMap::new();
        let mut rdr = csv::Reader::from_reader(routes_contents.as_bytes());

        for result in rdr.records() {
            if let Ok(record) = result {
                if let (Some(route_id), Some(route_color)) = (record.get(0), record.get(5)) {
                    if !route_color.is_empty() && route_color.len() == 6 {
                        color_map.insert(route_id.to_string(), route_color.to_string());
                    }
                }
            }
        }

        let mut shapes_map: HashMap<String, Vec<ShapePoint>> = HashMap::new();

        if let Ok(mut shapes_file) = archive.by_name("shapes.txt") {
            let mut shapes_contents = String::new();
            shapes_file.read_to_string(&mut shapes_contents).ok();
            drop(shapes_file);

            let mut shapes_rdr = csv::Reader::from_reader(shapes_contents.as_bytes());

            for result in shapes_rdr.records() {
                if let Ok(record) = result {
                    if let (Some(shape_id), Some(lat_str), Some(lon_str), Some(seq_str)) =
                        (record.get(0), record.get(1), record.get(2), record.get(3)) {
                        if let (Ok(lat), Ok(lon), Ok(seq)) =
                            (lat_str.parse::<f64>(), lon_str.parse::<f64>(), seq_str.parse::<u32>()) {

                            shapes_map.entry(shape_id.to_string())
                                .or_insert_with(Vec::new)
                                .push(ShapePoint {
                                    latitude: lat,
                                    longitude: lon,
                                    sequence: seq,
                                });
                        }
                    }
                }
            }

            for points in shapes_map.values_mut() {
                points.sort_by_key(|p| p.sequence);
            }

            println!("✓ Loaded {} shapes", shapes_map.len());
        }

        let mut route_to_shapes: HashMap<String, Vec<String>> = HashMap::new();

        if let Ok(mut trips_file) = archive.by_name("trips.txt") {
            let mut trips_contents = String::new();
            trips_file.read_to_string(&mut trips_contents).ok();
            drop(trips_file);

            let mut trips_rdr = csv::Reader::from_reader(trips_contents.as_bytes());

            for result in trips_rdr.records() {
                if let Ok(record) = result {
                    if let (Some(route_id), Some(shape_id)) = (record.get(0), record.get(6)) {
                        if !shape_id.is_empty() {
                            route_to_shapes.entry(route_id.to_string())
                                .or_insert_with(Vec::new)
                                .push(shape_id.to_string());
                        }
                    }
                }
            }

            for shape_ids in route_to_shapes.values_mut() {
                shape_ids.sort();
                shape_ids.dedup();
            }

            println!("✓ Mapped {} routes to shapes", route_to_shapes.len());
        }

        let mut stops_data = Vec::new();
        if let Ok(mut stops_file) = archive.by_name("stops.txt") {
            let mut contents = String::new();
            stops_file.read_to_string(&mut contents).ok();
            drop(stops_file);

            let mut stops_rdr = csv::Reader::from_reader(contents.as_bytes());

            for result in stops_rdr.records() {
                if let Ok(record) = result {
                    if let (Some(stop_id), Some(stop_name), Some(lat_str), Some(lon_str)) =
                        (record.get(0), record.get(2), record.get(4), record.get(5)) {
                        if let (Ok(lat), Ok(lon)) = (lat_str.parse::<f64>(), lon_str.parse::<f64>()) {
                            stops_data.push((
                                stop_id.to_string(),
                                stop_name.to_string(),
                                lat,
                                lon,
                            ));
                        }
                    }
                }
            }
        }

        let cache = GTFSCache {
            routes: color_map.clone(),
            stops: stops_data,
            shapes: shapes_map,
            route_to_shapes,
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            source: "TBM".to_string(),
        };

        if let Err(e) = cache.save() {
            eprintln!("⚠️  Warning: Could not save TBM GTFS cache: {}", e);
        }

        println!("✓ Loaded {} route colors", cache.routes.len());
        println!("✓ Cached {} stops for future use", cache.stops.len());

        Ok(cache)
    }

    fn load_gtfs_data(source: &str, max_age_days: u64) -> Result<GTFSCache> {
        if source == "TBM" {
            Self::download_and_read_gtfs()
        } else {
            Err(NVTError::ParseError(format!("Unknown GTFS source: {}", source)))
        }
    }

    // Helper methods for building network data
    pub fn build_stops(
        stops_data: Vec<(String, String, f64, f64, Vec<String>)>,
        alerts: Vec<AlertInfo>,
        real_time: Vec<RealTimeInfo>,
        trip_updates: Vec<gtfs_rt::TripUpdate>,
        lines_metadata: &[(String, String, String, Vec<(String, String)>)],
    ) -> Vec<Stop> {
        let line_destinations_map: HashMap<String, Vec<(String, String)>> = lines_metadata
            .iter()
            .filter_map(|(ref_, _, _, destinations)| {
                let line_id = Self::extract_line_id(ref_)?;
                Some((line_id.to_string(), destinations.clone()))
            })
            .collect();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let grace_period = 120;
        let cutoff_time = now - grace_period;

        let mut trip_updates_by_stop: HashMap<String, Vec<(String, Option<String>, Option<u32>, Option<i32>, Option<i64>)>> = HashMap::new();

        for trip_update in &trip_updates {
            let trip_id = trip_update.trip.trip_id.clone().unwrap_or_else(|| "Unknown".to_string());
            let route_id = trip_update.trip.route_id.clone();
            let direction_id = trip_update.trip.direction_id;

            for stu in &trip_update.stop_time_update {
                if let Some(stop_id_raw) = &stu.stop_id {
                    let delay = stu.arrival.as_ref().and_then(|a| a.delay)
                        .or_else(|| stu.departure.as_ref().and_then(|d| d.delay));
                    let time = stu.arrival.as_ref().and_then(|a| a.time)
                        .or_else(|| stu.departure.as_ref().and_then(|d| d.time))
                        .map(|t| t as i64);

                    if let Some(arrival_time) = time {
                        if arrival_time >= cutoff_time {
                            let data = (
                                trip_id.clone(),
                                route_id.clone(),
                                direction_id,
                                delay,
                                time,
                            );

                            trip_updates_by_stop
                                .entry(stop_id_raw.clone())
                                .or_insert_with(Vec::new)
                                .push(data.clone());

                            if let Some(extracted) = Self::extract_stop_id(stop_id_raw) {
                                if extracted != *stop_id_raw {
                                    trip_updates_by_stop
                                        .entry(extracted)
                                        .or_insert_with(Vec::new)
                                        .push(data);
                                }
                            }
                        }
                    }
                }
            }
        }

        stops_data
            .into_iter()
            .map(|(id, name, lat, lon, line_refs)| {
                let mut stop_rt: Vec<RealTimeInfo> = real_time
                    .iter()
                    .filter(|rt| {
                        rt.stop_id
                            .as_ref()
                            .map(|sid| sid == &id)
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect();

                if let Some(scheduled_arrivals) = trip_updates_by_stop.get(&id) {
                    for (trip_id, route_id, direction_id, delay, time) in scheduled_arrivals {
                        let destination = route_id.as_ref().and_then(|rid| {
                            line_destinations_map.get(rid).and_then(|destinations| {
                                direction_id.and_then(|dir_id| {
                                    destinations.iter()
                                        .find(|(dir_ref, _)| dir_ref == &dir_id.to_string())
                                        .map(|(_, place)| place.clone())
                                })
                            })
                        });

                        stop_rt.push(RealTimeInfo {
                            vehicle_id: "scheduled".to_string(),
                            trip_id: trip_id.clone(),
                            route_id: route_id.clone(),
                            direction_id: *direction_id,
                            destination,
                            latitude: lat,
                            longitude: lon,
                            stop_id: Some(id.clone()),
                            timestamp: *time,
                            delay: *delay,
                        });
                    }
                }

                stop_rt.retain(|rt| {
                    if let Some(ts) = rt.timestamp {
                        ts >= cutoff_time
                    } else {
                        true
                    }
                });

                stop_rt.sort_by_key(|rt| rt.timestamp.unwrap_or(i64::MAX));

                const MAX_ARRIVALS_PER_STOP: usize = 10;
                if stop_rt.len() > MAX_ARRIVALS_PER_STOP {
                    stop_rt.truncate(MAX_ARRIVALS_PER_STOP);
                }

                let stop_alerts: Vec<AlertInfo> = alerts
                    .iter()
                    .filter(|alert| alert.stop_ids.contains(&id))
                    .cloned()
                    .collect();

                Stop {
                    stop_id: id,
                    stop_name: name,
                    latitude: lat,
                    longitude: lon,
                    lines: line_refs,
                    alerts: stop_alerts,
                    real_time: stop_rt,
                }
            })
            .collect()
    }

    pub fn build_lines(
        lines_data: Vec<(String, String, String, Vec<(String, String)>)>,
        alerts: Vec<AlertInfo>,
        real_time: Vec<RealTimeInfo>,
        gtfs_cache: &GTFSCache,
    ) -> Vec<Line> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let cutoff_time = now - 120;

        lines_data
            .into_iter()
            .map(|(line_ref_str, name, code, destinations)| {
                let line_id_str = Self::extract_line_id(&line_ref_str)
                    .unwrap_or("")
                    .to_string();

                let color = gtfs_cache.routes
                    .get(&line_id_str)
                    .cloned()
                    .unwrap_or_else(|| "808080".to_string());

                let shape_ids = gtfs_cache.route_to_shapes
                    .get(&line_id_str)
                    .cloned()
                    .unwrap_or_default();

                let line_alerts: Vec<AlertInfo> = alerts
                    .iter()
                    .filter(|alert| {
                        alert.route_ids.contains(&code) ||
                            alert.route_ids.contains(&line_id_str)
                    })
                    .cloned()
                    .collect();

                let mut line_rt: Vec<RealTimeInfo> = real_time
                    .iter()
                    .filter(|rt| {
                        rt.route_id
                            .as_ref()
                            .map(|route| route == &line_id_str)
                            .unwrap_or(false)
                    })
                    .filter(|rt| {
                        if let Some(ts) = rt.timestamp {
                            ts >= cutoff_time
                        } else {
                            true
                        }
                    })
                    .cloned()
                    .collect();

                line_rt.sort_by_key(|rt| rt.timestamp.unwrap_or(i64::MAX));

                Line {
                    line_ref: line_ref_str,
                    line_name: name,
                    line_code: code,
                    route_id: line_id_str,
                    destinations,
                    alerts: line_alerts,
                    real_time: line_rt,
                    color,
                    shape_ids,
                    operator: "TBM".to_string(),
                }
            })
            .collect()
    }

    fn extract_stop_id(full_id: &str) -> Option<String> {
        if full_id.contains("BP:") {
            full_id
                .split("BP:")
                .nth(1)?
                .split(':')
                .next()
                .map(String::from)
        } else if full_id.contains(':') {
            let parts: Vec<&str> = full_id.split(':').collect();
            if parts.len() >= 2 {
                Some(parts[parts.len() - 2].to_string())
            } else {
                Some(full_id.to_string())
            }
        } else {
            Some(full_id.to_string())
        }
    }

    pub fn extract_line_id(line_ref: &str) -> Option<&str> {
        line_ref.split(':').nth(2)
    }

    pub fn format_timestamp_full(timestamp: i64) -> String {
        match Utc.timestamp_opt(timestamp, 0).single() {
            Some(dt) => {
                let paris_time = dt.with_timezone(&Paris);
                paris_time.format("%Y-%m-%d %H:%M:%S").to_string()
            }
            None => format!("Invalid timestamp: {}", timestamp),
        }
    }

    pub fn get_current_timestamp() -> i64 {
        Utc::now().timestamp()
    }

    pub fn get_cache_stats(cache: &CachedNetworkData) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let static_age = now.saturating_sub(cache.last_static_update);
        let dynamic_age = now.saturating_sub(cache.last_dynamic_update);

        format!(
            "📊 Cache Statistics:\n\
             • TBM: {} stops, {} lines\n\
             • TransGironde: {} stops, {} lines\n\
             • SNCF: {} stops, {} lines\n\
             • TBM Colors: {} | TBM Shapes: {}\n\
             • TransGironde Colors: {} | TransGironde Shapes: {}\n\
             • SNCF Colors: {} | SNCF Shapes: {}\n\
             • Vehicles tracked: {} | Alerts: {}\n\
             • Static data age: {}s | Dynamic data age: {}s\n\
             • Last update: {}",
            cache.tbm_stops_metadata.len(),
            cache.tbm_lines_metadata.len(),
            cache.transgironde_stops.len(),
            cache.transgironde_lines.len(),
            cache.sncf_stops.len(),
            cache.sncf_lines.len(),
            cache.tbm_gtfs_cache.routes.len(),
            cache.tbm_gtfs_cache.shapes.len(),
            cache.transgironde_gtfs_cache.routes.len(),
            cache.transgironde_gtfs_cache.shapes.len(),
            cache.sncf_gtfs_cache.routes.len(),
            cache.sncf_gtfs_cache.shapes.len(),
            cache.real_time.len(),
            cache.alerts.len(),
            static_age,
            dynamic_age,
            Self::format_timestamp_full(cache.last_dynamic_update as i64)
        )
    }
}