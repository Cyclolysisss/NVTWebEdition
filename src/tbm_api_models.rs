// API models and data fetching for TBM (Transports Bordeaux Métropole), New-Aquitaine Regional Networks, and SNCF
// TBM Official website: https://www.infotbm.com/
// New-Aquitaine Region Official website: https://www.nouvelle-aquitaine.fr/
//
// TBM API Endpoints:
// - Stop Discovery SIRI-Lite: https://bdx.mecatran.com/utw/ws/siri/2.0/bordeaux/stoppoints-discovery.json
// - Lines Discovery SIRI-Lite: https://bdx.mecatran.com/utw/ws/siri/2.0/bordeaux/lines-discovery.json
// - GTFS-RT Vehicles: https://bdx.mecatran.com/utw/ws/gtfsfeed/vehicles/bordeaux
// - GTFS-RT Alerts: https://bdx.mecatran.com/utw/ws/gtfsfeed/alerts/bordeaux
// - GTFS-RT Trip Updates: https://bdx.mecatran.com/utw/ws/gtfsfeed/realtime/bordeaux
//
// New-Aquitaine Regional Networks Data:
// - GTFS Static (Aggregated): https://www.pigma.org/public/opendata/nouvelle_aquitaine_mobilites/publication/naq-aggregated-gtfs.zip
// - Includes 50+ transit operators across the New-Aquitaine region

use reqwest::blocking;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
    pub current_stop_sequence: Option<u32>,
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
pub struct StopTime {
    pub trip_id: String,
    pub arrival_time: String,
    pub departure_time: String,
    pub stop_id: String,
    pub stop_sequence: u32,
    pub stop_headsign: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trip {
    pub trip_id: String,
    pub route_id: String,
    pub service_id: String,
    pub trip_headsign: Option<String>,
    pub direction_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCalendar {
    pub service_id: String,
    pub monday: bool,
    pub tuesday: bool,
    pub wednesday: bool,
    pub thursday: bool,
    pub friday: bool,
    pub saturday: bool,
    pub sunday: bool,
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarDate {
    pub service_id: String,
    pub date: String,
    pub exception_type: u32, // 1 = service added, 2 = service removed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agency {
    pub agency_id: String,
    pub agency_name: String,
    pub agency_url: String,
    pub agency_timezone: String,
    pub agency_phone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub from_stop_id: String,
    pub to_stop_id: String,
    pub transfer_type: u32,
    pub min_transfer_time: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledArrival {
    pub trip_id: String,
    pub route_id: String,
    pub line_code: String,
    pub line_color: String,
    pub arrival_time: String,
    pub departure_time: String,
    pub destination: Option<String>,
    pub stop_headsign: Option<String>,
    pub operator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleDetails {
    pub vehicle_id: String,
    pub trip_id: String,
    pub route_id: Option<String>,
    pub line_code: String,
    pub line_name: String,
    pub line_color: String,
    pub operator: String,
    pub destination: Option<String>,
    pub current_stop: Option<Stop>,
    pub next_stop: Option<Stop>,
    pub previous_stop: Option<Stop>,
    pub latitude: f64,
    pub longitude: f64,
    pub timestamp: Option<i64>,
    pub delay: Option<i32>,
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
    pub operator: String, // Operator name (e.g., "TBM", "YELO", "Calibus (Libourne)", "STCLM (Limoges Métropole)", etc.)
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkData {
    pub stops: Vec<Stop>,
    pub lines: Vec<Line>,
    pub shapes: HashMap<String, Vec<ShapePoint>>,
}

// ============================================================================
// GTFS Cache Structure (15-day persistence for TBM, 30-day for New-Aquitaine and SNCF)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GTFSCache {
    pub routes: HashMap<String, String>,
    pub stops: Vec<(String, String, f64, f64)>,
    pub shapes: HashMap<String, Vec<ShapePoint>>,
    pub route_to_shapes: HashMap<String, Vec<String>>,
    pub stop_times: HashMap<String, Vec<StopTime>>, // key: stop_id, value: list of stop times
    pub trips: HashMap<String, Trip>, // key: trip_id, value: trip info
    pub calendar: HashMap<String, ServiceCalendar>, // key: service_id
    pub calendar_dates: HashMap<String, Vec<CalendarDate>>, // key: service_id
    pub agencies: HashMap<String, Agency>, // key: agency_id, value: agency info
    pub route_agencies: HashMap<String, String>, // key: route_id, value: agency_id
    pub transfers: Vec<Transfer>,
    pub cached_at: u64,
    pub source: String, // "TBM", "NewAquitaine", or "SNCF"
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

    // New-Aquitaine Regional Networks Data (variable names kept as "transgironde" for backward compatibility)
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

        // Add New-Aquitaine stops
        all_stops.extend(self.transgironde_stops.clone());

        // Add SNCF stops
        all_stops.extend(self.sncf_stops.clone());

        let mut all_lines = NVTModels::build_lines(
            self.tbm_lines_metadata.clone(),
            self.alerts.clone(),
            self.real_time.clone(),
            &self.tbm_gtfs_cache,
        );

        // Add New-Aquitaine lines
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
    const TRANSGIRONDE_GTFS_URL: &'static str = "https://www.pigma.org/public/opendata/nouvelle_aquitaine_mobilites/publication/naq-aggregated-gtfs.zip";
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
                stop_times: HashMap::new(),
                trips: HashMap::new(),
                calendar: HashMap::new(),
                calendar_dates: HashMap::new(),
                agencies: HashMap::new(),
                route_agencies: HashMap::new(),
                transfers: Vec::new(),
                cached_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                source: "TBM".to_string(),
            }
        });
        println!("   ✓ Loaded {} TBM line colors", tbm_gtfs_cache.routes.len());

        // Load TransGironde data
        println!("\n🚌 Loading New-Aquitaine data...");
        let (transgironde_stops, transgironde_lines, transgironde_gtfs_cache) =
            Self::load_transgironde_data().unwrap_or_else(|e| {
                println!("   ⚠️  Warning: Could not load New-Aquitaine data ({})", e);
                println!("   Continuing without New-Aquitaine...");
                (Vec::new(), Vec::new(), GTFSCache {
                    routes: HashMap::new(),
                    stops: Vec::new(),
                    shapes: HashMap::new(),
                    route_to_shapes: HashMap::new(),
                    stop_times: HashMap::new(),
                    trips: HashMap::new(),
                    calendar: HashMap::new(),
                    calendar_dates: HashMap::new(),
                    agencies: HashMap::new(),
                    route_agencies: HashMap::new(),
                    transfers: Vec::new(),
                    cached_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    source: "NewAquitaine".to_string(),
                })
            });
        println!("   ✓ Loaded {} New-Aquitaine stops", transgironde_stops.len());
        println!("   ✓ Loaded {} New-Aquitaine lines", transgironde_lines.len());
        println!("   ✓ Loaded {} New-Aquitaine shapes", transgironde_gtfs_cache.shapes.len());

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
                    stop_times: HashMap::new(),
                    trips: HashMap::new(),
                    calendar: HashMap::new(),
                    calendar_dates: HashMap::new(),
                    agencies: HashMap::new(),
                    route_agencies: HashMap::new(),
                    transfers: Vec::new(),
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
        println!("  • New-Aquitaine: {} stops, {} lines", transgironde_stops.len(), transgironde_lines.len());
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
        // Fetch TBM data
        cache.alerts = Self::fetch_alerts().unwrap_or_else(|e| {
            eprintln!("⚠️  Warning: Could not fetch TBM alerts ({})", e);
            cache.alerts.clone()
        });

        cache.real_time = Self::fetch_vehicle_positions().unwrap_or_else(|e| {
            eprintln!("⚠️  Warning: Could not fetch TBM vehicle positions ({})", e);
            cache.real_time.clone()
        });

        cache.trip_updates = Self::fetch_trip_updates().unwrap_or_else(|e| {
            eprintln!("⚠️  Warning: Could not fetch TBM trip updates ({})", e);
            cache.trip_updates.clone()
        });

        // Fetch SNCF real-time data
        let sncf_alerts = Self::fetch_sncf_alerts().unwrap_or_else(|e| {
            eprintln!("⚠️  Warning: Could not fetch SNCF alerts ({})", e);
            Vec::new()
        });

        let sncf_trip_updates = Self::fetch_sncf_trip_updates().unwrap_or_else(|e| {
            eprintln!("⚠️  Warning: Could not fetch SNCF trip updates ({})", e);
            Vec::new()
        });

        // Merge SNCF data with TBM data
        cache.alerts.extend(sncf_alerts);
        cache.trip_updates.extend(sncf_trip_updates);

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
    // New-Aquitaine Regional Networks GTFS Loading
    // (Function name kept as "load_transgironde_data" for backward compatibility)
    // ============================================================================

    fn load_transgironde_data() -> Result<(Vec<Stop>, Vec<Line>, GTFSCache)> {
        if let Some(cache) = GTFSCache::load("NewAquitaine", 30) {
            return Self::parse_transgironde_from_cache(cache);
        }

        println!("📥 Downloading New-Aquitaine GTFS data...");

        let client = blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(Self::REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| NVTError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let response = client.get(Self::TRANSGIRONDE_GTFS_URL)
            .send()
            .map_err(|e| NVTError::NetworkError(format!("Failed to download New-Aquitaine GTFS: {}", e)))?;

        if !response.status().is_success() {
            return Err(NVTError::NetworkError(format!("Download failed with status: {}", response.status())));
        }

        let zip_bytes = response.bytes()
            .map_err(|e| NVTError::NetworkError(format!("Failed to read GTFS zip: {}", e)))?;

        println!("✓ Downloaded {} KB, extracting...", zip_bytes.len() / 1024);

        let cursor = Cursor::new(zip_bytes);
        let mut archive = ZipArchive::new(cursor)
            .map_err(|e| NVTError::ParseError(format!("Failed to open GTFS zip: {}", e)))?;

        // Parse agency.txt first to get operator information
        let agencies = Self::parse_agencies(&mut archive)?;
        println!("   ✓ Parsed {} agencies", agencies.len());

        // Parse routes.txt with agency_id
        let (routes, route_agencies) = Self::parse_transgironde_routes(&mut archive)?;
        println!("   ✓ Parsed {} New-Aquitaine routes", routes.len());

        // Parse stops.txt
        let stops_data = Self::parse_transgironde_stops(&mut archive)?;
        println!("   ✓ Parsed {} New-Aquitaine stops", stops_data.len());

        // Parse shapes.txt
        let shapes = Self::parse_transgironde_shapes(&mut archive)?;
        println!("   ✓ Parsed {} New-Aquitaine shapes", shapes.len());

        // Parse trips.txt to map routes to shapes
        let route_to_shapes = Self::parse_transgironde_trips(&mut archive)?;
        println!("   ✓ Mapped {} routes to shapes", route_to_shapes.len());

        // Parse stop_times.txt for schedule predictions
        let stop_times = Self::parse_stop_times(&mut archive)?;
        println!("   ✓ Parsed {} stop time entries", stop_times.values().map(|v| v.len()).sum::<usize>());

        // Parse trips.txt for trip information
        let trips = Self::parse_trips_info(&mut archive)?;
        println!("   ✓ Parsed {} trips", trips.len());

        // Parse calendar.txt for service schedules
        let calendar = Self::parse_calendar(&mut archive)?;
        println!("   ✓ Parsed {} calendar services", calendar.len());

        // Parse calendar_dates.txt for exceptions
        let calendar_dates = Self::parse_calendar_dates(&mut archive)?;
        println!("   ✓ Parsed {} calendar date exceptions", calendar_dates.values().map(|v| v.len()).sum::<usize>());

        // Parse transfers.txt
        let transfers = Self::parse_transfers(&mut archive)?;
        println!("   ✓ Parsed {} transfers", transfers.len());

        let gtfs_cache = GTFSCache {
            routes,
            stops: stops_data.clone(),
            shapes: shapes.clone(),
            route_to_shapes: route_to_shapes.clone(),
            stop_times,
            trips,
            calendar,
            calendar_dates,
            agencies,
            route_agencies,
            transfers,
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            source: "NewAquitaine".to_string(),
        };

        if let Err(e) = gtfs_cache.save() {
            eprintln!("⚠️  Warning: Could not save TransGironde cache: {}", e);
        }

        Self::parse_transgironde_from_cache(gtfs_cache)
    }

    fn parse_agencies(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<HashMap<String, Agency>> {
        let mut agencies_map = HashMap::new();

        if let Ok(mut agencies_file) = archive.by_name("agency.txt") {
            let mut agencies_contents = String::new();
            agencies_file.read_to_string(&mut agencies_contents).ok();
            drop(agencies_file);

            let mut rdr = csv::Reader::from_reader(agencies_contents.as_bytes());

            for result in rdr.records() {
                if let Ok(record) = result {
                    // agency_id,agency_name,agency_url,agency_timezone,agency_phone
                    if let (Some(agency_id), Some(agency_name), Some(agency_url), Some(agency_timezone), Some(agency_phone)) =
                        (record.get(0), record.get(1), record.get(2), record.get(3), record.get(4)) {
                        agencies_map.insert(agency_id.to_string(), Agency {
                            agency_id: agency_id.to_string(),
                            agency_name: agency_name.to_string(),
                            agency_url: agency_url.to_string(),
                            agency_timezone: agency_timezone.to_string(),
                            agency_phone: agency_phone.to_string(),
                        });
                    }
                }
            }
        }

        Ok(agencies_map)
    }

    fn parse_transgironde_routes(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<(HashMap<String, String>, HashMap<String, String>)> {
        let mut routes_file = archive.by_name("routes.txt")
            .map_err(|e| NVTError::FileError(format!("routes.txt not found: {}", e)))?;

        let mut routes_contents = String::new();
        routes_file.read_to_string(&mut routes_contents)
            .map_err(|e| NVTError::FileError(format!("Failed to read routes.txt: {}", e)))?;

        drop(routes_file);

        let mut color_map = HashMap::new();
        let mut route_agencies = HashMap::new();
        let mut rdr = csv::Reader::from_reader(routes_contents.as_bytes());

        for result in rdr.records() {
            if let Ok(record) = result {
                // route_id,agency_id,route_short_name,route_long_name,route_desc,route_type,route_url,route_color,route_text_color
                if let Some(route_id) = record.get(0) {
                    // Store agency_id if present
                    if let Some(agency_id) = record.get(1) {
                        if !agency_id.is_empty() {
                            route_agencies.insert(route_id.to_string(), agency_id.to_string());
                        }
                    }
                    
                    // Store route color
                    if let Some(route_color) = record.get(7) {
                        if !route_color.is_empty() && route_color.len() == 6 {
                            color_map.insert(route_id.to_string(), route_color.to_string());
                        }
                    }
                }
            }
        }

        Ok((color_map, route_agencies))
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
                    // shape_id,shape_pt_sequence,shape_pt_lat,shape_pt_lon
                    if let (Some(shape_id), Some(seq_str), Some(lat_str), Some(lon_str)) =
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

    fn parse_stop_times(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<HashMap<String, Vec<StopTime>>> {
        let mut stop_times_map: HashMap<String, Vec<StopTime>> = HashMap::new();

        if let Ok(mut stop_times_file) = archive.by_name("stop_times.txt") {
            let mut contents = String::new();
            stop_times_file.read_to_string(&mut contents).ok();
            drop(stop_times_file);

            let mut rdr = csv::Reader::from_reader(contents.as_bytes());

            for result in rdr.records() {
                if let Ok(record) = result {
                    // trip_id,arrival_time,departure_time,stop_id,stop_sequence,stop_headsign,pickup_type,drop_off_type,shape_dist_traveled
                    if let (Some(trip_id), Some(arrival_time), Some(departure_time), Some(stop_id), Some(stop_sequence)) =
                        (record.get(0), record.get(1), record.get(2), record.get(3), record.get(4)) {
                        if let Ok(sequence) = stop_sequence.parse::<u32>() {
                            let stop_time = StopTime {
                                trip_id: trip_id.to_string(),
                                arrival_time: arrival_time.to_string(),
                                departure_time: departure_time.to_string(),
                                stop_id: stop_id.to_string(),
                                stop_sequence: sequence,
                                stop_headsign: record.get(5).map(|s| s.to_string()).filter(|s| !s.is_empty()),
                            };

                            stop_times_map.entry(stop_id.to_string())
                                .or_insert_with(Vec::new)
                                .push(stop_time);
                        }
                    }
                }
            }

            // Sort stop times by arrival time for each stop
            for times in stop_times_map.values_mut() {
                times.sort_by(|a, b| a.arrival_time.cmp(&b.arrival_time));
            }
        }

        Ok(stop_times_map)
    }

    fn parse_trips_info(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<HashMap<String, Trip>> {
        let mut trips_map: HashMap<String, Trip> = HashMap::new();

        if let Ok(mut trips_file) = archive.by_name("trips.txt") {
            let mut contents = String::new();
            trips_file.read_to_string(&mut contents).ok();
            drop(trips_file);

            let mut rdr = csv::Reader::from_reader(contents.as_bytes());

            for result in rdr.records() {
                if let Ok(record) = result {
                    // route_id,service_id,trip_id,trip_headsign,direction_id,block_id,shape_id,wheelchair_accessible,bikes_allowed
                    if let (Some(route_id), Some(service_id), Some(trip_id)) =
                        (record.get(0), record.get(1), record.get(2)) {
                        let trip = Trip {
                            trip_id: trip_id.to_string(),
                            route_id: route_id.to_string(),
                            service_id: service_id.to_string(),
                            trip_headsign: record.get(3).map(|s| s.to_string()).filter(|s| !s.is_empty()),
                            direction_id: record.get(4).and_then(|s| s.parse::<u32>().ok()),
                        };

                        trips_map.insert(trip_id.to_string(), trip);
                    }
                }
            }
        }

        Ok(trips_map)
    }

    fn parse_calendar(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<HashMap<String, ServiceCalendar>> {
        let mut calendar_map: HashMap<String, ServiceCalendar> = HashMap::new();

        if let Ok(mut calendar_file) = archive.by_name("calendar.txt") {
            let mut contents = String::new();
            calendar_file.read_to_string(&mut contents).ok();
            drop(calendar_file);

            let mut rdr = csv::Reader::from_reader(contents.as_bytes());

            for result in rdr.records() {
                if let Ok(record) = result {
                    // service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date
                    if let (Some(service_id), Some(mon), Some(tue), Some(wed), Some(thu), Some(fri), Some(sat), Some(sun), Some(start), Some(end)) =
                        (record.get(0), record.get(1), record.get(2), record.get(3), record.get(4), record.get(5), record.get(6), record.get(7), record.get(8), record.get(9)) {
                        
                        let calendar = ServiceCalendar {
                            service_id: service_id.to_string(),
                            monday: mon == "1",
                            tuesday: tue == "1",
                            wednesday: wed == "1",
                            thursday: thu == "1",
                            friday: fri == "1",
                            saturday: sat == "1",
                            sunday: sun == "1",
                            start_date: start.to_string(),
                            end_date: end.to_string(),
                        };

                        calendar_map.insert(service_id.to_string(), calendar);
                    }
                }
            }
        }

        Ok(calendar_map)
    }

    fn parse_calendar_dates(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<HashMap<String, Vec<CalendarDate>>> {
        let mut calendar_dates_map: HashMap<String, Vec<CalendarDate>> = HashMap::new();

        if let Ok(mut calendar_dates_file) = archive.by_name("calendar_dates.txt") {
            let mut contents = String::new();
            calendar_dates_file.read_to_string(&mut contents).ok();
            drop(calendar_dates_file);

            let mut rdr = csv::Reader::from_reader(contents.as_bytes());

            for result in rdr.records() {
                if let Ok(record) = result {
                    // service_id,date,exception_type
                    if let (Some(service_id), Some(date), Some(exception_type)) =
                        (record.get(0), record.get(1), record.get(2)) {
                        if let Ok(exc_type) = exception_type.parse::<u32>() {
                            let calendar_date = CalendarDate {
                                service_id: service_id.to_string(),
                                date: date.to_string(),
                                exception_type: exc_type,
                            };

                            calendar_dates_map.entry(service_id.to_string())
                                .or_insert_with(Vec::new)
                                .push(calendar_date);
                        }
                    }
                }
            }
        }

        Ok(calendar_dates_map)
    }

    fn parse_transfers(archive: &mut ZipArchive<Cursor<bytes::Bytes>>) -> Result<Vec<Transfer>> {
        let mut transfers = Vec::new();

        if let Ok(mut transfers_file) = archive.by_name("transfers.txt") {
            let mut contents = String::new();
            transfers_file.read_to_string(&mut contents).ok();
            drop(transfers_file);

            let mut rdr = csv::Reader::from_reader(contents.as_bytes());

            for result in rdr.records() {
                if let Ok(record) = result {
                    // from_stop_id,to_stop_id,transfer_type,min_transfer_time
                    if let (Some(from_stop_id), Some(to_stop_id), Some(transfer_type)) =
                        (record.get(0), record.get(1), record.get(2)) {
                        if let Ok(trans_type) = transfer_type.parse::<u32>() {
                            let min_transfer_time = record.get(3)
                                .and_then(|s| s.parse::<u32>().ok());

                            transfers.push(Transfer {
                                from_stop_id: from_stop_id.to_string(),
                                to_stop_id: to_stop_id.to_string(),
                                transfer_type: trans_type,
                                min_transfer_time,
                            });
                        }
                    }
                }
            }
        }

        Ok(transfers)
    }

    fn parse_transgironde_from_cache(cache: GTFSCache) -> Result<(Vec<Stop>, Vec<Line>, GTFSCache)> {
        // Build a map of stop_id -> set of route_ids that serve this stop
        let mut stop_to_routes: HashMap<String, HashSet<String>> = HashMap::new();
        
        // Use stop_times and trips to determine which routes serve which stops
        for (stop_id, stop_times) in &cache.stop_times {
            for stop_time in stop_times {
                if let Some(trip) = cache.trips.get(&stop_time.trip_id) {
                    stop_to_routes.entry(stop_id.clone())
                        .or_insert_with(HashSet::new)
                        .insert(trip.route_id.clone());
                }
            }
        }
        
        let mut stops = Vec::new();

        // Create stops with properly populated lines arrays
        for (stop_id, stop_name, lat, lon) in &cache.stops {
            let lines: Vec<String> = stop_to_routes.get(stop_id)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default();
            
            stops.push(Stop {
                stop_id: stop_id.clone(),
                stop_name: stop_name.clone(),
                latitude: *lat,
                longitude: *lon,
                lines, // Now populated with actual route_ids (unique by nature of HashSet)
                alerts: Vec::new(),
                real_time: Vec::new(),
            });
        }

        // Create lines from routes
        let mut lines = Vec::new();
        for (route_id, color) in &cache.routes {
            // Get the agency_id for this route, if available
            let agency_id = cache.route_agencies.get(route_id);
            
            // Get the operator name from the agency, or use a default
            let operator = if let Some(aid) = agency_id {
                if let Some(agency) = cache.agencies.get(aid) {
                    // Extract short operator name from agency_name
                    // Format: "Calibus (Libourne)" or "TBM (Bordeaux Métropole)"
                    agency.agency_name.clone()
                } else {
                    "New-Aquitaine".to_string()
                }
            } else {
                "New-Aquitaine".to_string()
            };
            
            // Extract route short name from route_id
            // Format: "CA_DU_LIBOURNAIS:Line:XXX" -> "XXX"
            let line_code = route_id.split(':').last().unwrap_or(route_id);

            let shape_ids = cache.route_to_shapes.get(route_id)
                .cloned()
                .unwrap_or_default();

            lines.push(Line {
                line_ref: route_id.clone(),
                line_name: format!("{} {}", operator, line_code),
                line_code: line_code.to_string(),
                route_id: route_id.clone(),
                destinations: Vec::new(),
                alerts: Vec::new(),
                real_time: Vec::new(),
                color: color.clone(),
                shape_ids,
                operator,
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

        // Parse stop_times.txt for schedule predictions
        let stop_times = Self::parse_stop_times(&mut archive)?;
        println!("   ✓ Parsed {} stop time entries", stop_times.values().map(|v| v.len()).sum::<usize>());

        // Parse trips.txt for trip information
        let trips = Self::parse_trips_info(&mut archive)?;
        println!("   ✓ Parsed {} trips", trips.len());

        // Parse calendar.txt for service schedules
        let calendar = Self::parse_calendar(&mut archive)?;
        println!("   ✓ Parsed {} calendar services", calendar.len());

        // Parse calendar_dates.txt for exceptions
        let calendar_dates = Self::parse_calendar_dates(&mut archive)?;
        println!("   ✓ Parsed {} calendar date exceptions", calendar_dates.values().map(|v| v.len()).sum::<usize>());

        let gtfs_cache = GTFSCache {
            routes,
            stops: stops_data.clone(),
            shapes: shapes.clone(),
            route_to_shapes: route_to_shapes.clone(),
            stop_times,
            trips,
            calendar,
            calendar_dates,
            agencies: HashMap::new(),
            route_agencies: HashMap::new(),
            transfers: Vec::new(),
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
        // Build a map of stop_id -> set of route_ids that serve this stop
        let mut stop_to_routes: HashMap<String, HashSet<String>> = HashMap::new();
        
        // Use stop_times and trips to determine which routes serve which stops
        for (stop_id, stop_times) in &cache.stop_times {
            for stop_time in stop_times {
                if let Some(trip) = cache.trips.get(&stop_time.trip_id) {
                    stop_to_routes.entry(stop_id.clone())
                        .or_insert_with(HashSet::new)
                        .insert(trip.route_id.clone());
                }
            }
        }
        
        let mut stops = Vec::new();

        // Create stops with properly populated lines arrays
        for (stop_id, stop_name, lat, lon) in &cache.stops {
            let lines: Vec<String> = stop_to_routes.get(stop_id)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default();
            
            stops.push(Stop {
                stop_id: stop_id.clone(),
                stop_name: stop_name.clone(),
                latitude: *lat,
                longitude: *lon,
                lines, // Now populated with actual route_ids (unique by nature of HashSet)
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

    fn create_http_client() -> Result<blocking::Client> {
        blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(Self::REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| NVTError::NetworkError(format!("Failed to create HTTP client: {}", e)))
    }

    fn fetch_alerts() -> Result<Vec<AlertInfo>> {
        let url = format!(
            "{}/gtfsfeed/alerts/bordeaux?apiKey={}",
            Self::BASE_URL,
            Self::API_KEY
        );

        let client = Self::create_http_client()?;

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

        let client = Self::create_http_client()?;

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
                    let current_stop_sequence = vehicle.current_stop_sequence;
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
                        current_stop_sequence,
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

        let client = Self::create_http_client()?;

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

    fn fetch_sncf_trip_updates() -> Result<Vec<gtfs_rt::TripUpdate>> {
        let client = Self::create_http_client()?;

        let response = client.get(Self::SNCF_GTFS_RT_TRIP_UPDATES_URL)
            .send()
            .map_err(|e| NVTError::NetworkError(format!("Failed to fetch SNCF trip updates: {}", e)))?;

        if !response.status().is_success() {
            return Err(NVTError::NetworkError(format!("SNCF trip updates request failed with status: {}", response.status())));
        }

        let body = response.bytes()
            .map_err(|e| NVTError::NetworkError(format!("Failed to read SNCF trip updates response: {}", e)))?;

        let feed = FeedMessage::decode(&*body)
            .map_err(|e| NVTError::ParseError(format!("Failed to decode SNCF trip updates feed: {}", e)))?;

        let updates = feed
            .entity
            .into_iter()
            .filter_map(|entity| entity.trip_update)
            .collect();

        Ok(updates)
    }

    fn fetch_sncf_alerts() -> Result<Vec<AlertInfo>> {
        let client = Self::create_http_client()?;

        let response = client.get(Self::SNCF_GTFS_RT_SERVICE_ALERTS_URL)
            .send()
            .map_err(|e| NVTError::NetworkError(format!("Failed to fetch SNCF alerts: {}", e)))?;

        if !response.status().is_success() {
            return Err(NVTError::NetworkError(format!("SNCF alerts request failed with status: {}", response.status())));
        }

        let body = response.bytes()
            .map_err(|e| NVTError::NetworkError(format!("Failed to read SNCF alerts response: {}", e)))?;

        let feed = FeedMessage::decode(&*body)
            .map_err(|e| NVTError::ParseError(format!("Failed to decode SNCF alerts feed: {}", e)))?;

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

        // Parse stop_times.txt for schedule predictions
        let stop_times = Self::parse_stop_times(&mut archive)?;
        println!("✓ Parsed {} stop time entries", stop_times.values().map(|v| v.len()).sum::<usize>());

        // Parse trips.txt for trip information
        let trips = Self::parse_trips_info(&mut archive)?;
        println!("✓ Parsed {} trips", trips.len());

        // Parse calendar.txt for service schedules
        let calendar = Self::parse_calendar(&mut archive)?;
        println!("✓ Parsed {} calendar services", calendar.len());

        // Parse calendar_dates.txt for exceptions
        let calendar_dates = Self::parse_calendar_dates(&mut archive)?;
        println!("✓ Parsed {} calendar date exceptions", calendar_dates.values().map(|v| v.len()).sum::<usize>());

        let cache = GTFSCache {
            routes: color_map.clone(),
            stops: stops_data,
            shapes: shapes_map,
            route_to_shapes,
            stop_times,
            trips,
            calendar,
            calendar_dates,
            agencies: HashMap::new(),
            route_agencies: HashMap::new(),
            transfers: Vec::new(),
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

    fn load_gtfs_data(source: &str, _max_age_days: u64) -> Result<GTFSCache> {
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
                            current_stop_sequence: None,
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
             • New-Aquitaine: {} stops, {} lines\n\
             • SNCF: {} stops, {} lines\n\
             • TBM Colors: {} | TBM Shapes: {}\n\
             • New-Aquitaine Colors: {} | New-Aquitaine Shapes: {}\n\
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

    /// Get scheduled arrivals for a stop based on GTFS data
    pub fn get_scheduled_arrivals(
        stop_id: &str,
        cache: &CachedNetworkData,
        max_results: usize,
    ) -> Vec<ScheduledArrival> {
        use chrono::{Local, Datelike, Timelike};
        
        const SECONDS_PER_HOUR: u32 = 3600;
        const SECONDS_PER_MINUTE: u32 = 60;
        const SECONDS_IN_DAY: u32 = 86400;
        const LATE_EVENING_THRESHOLD: u32 = 79200; // 22:00:00
        
        let now = Local::now();
        let today_date = format!("{}{:02}{:02}", now.year(), now.month(), now.day());
        let current_seconds = now.hour() * SECONDS_PER_HOUR + now.minute() * SECONDS_PER_MINUTE + now.second();
        
        let weekday_num = now.weekday().num_days_from_monday(); // 0 = Monday, 6 = Sunday
        
        let mut scheduled_arrivals = Vec::new();
        
        // Check all three GTFS caches
        let gtfs_caches = vec![
            (&cache.tbm_gtfs_cache, "TBM"),
            (&cache.transgironde_gtfs_cache, "TransGironde"),
            (&cache.sncf_gtfs_cache, "SNCF"),
        ];
        
        for (gtfs_cache, operator) in gtfs_caches {
            // Get stop times for this stop
            if let Some(stop_times) = gtfs_cache.stop_times.get(stop_id) {
                for stop_time in stop_times {
                    // Get trip info
                    if let Some(trip) = gtfs_cache.trips.get(&stop_time.trip_id) {
                        // Check if service is active today
                        if !Self::is_service_active(
                            &trip.service_id,
                            &today_date,
                            weekday_num,
                            &gtfs_cache.calendar,
                            &gtfs_cache.calendar_dates,
                        ) {
                            continue;
                        }
                        
                        // Parse arrival time
                        if let Some(arrival_seconds) = Self::parse_gtfs_time(&stop_time.arrival_time) {
                            // Handle next-day services (times >= 24:00:00)
                            // Only include future arrivals within the next 2 hours window
                            let is_future = if arrival_seconds >= SECONDS_IN_DAY {
                                // Next-day service (e.g., 25:30:00)
                                // Only show if current time is late enough (e.g., after 22:00)
                                current_seconds >= LATE_EVENING_THRESHOLD
                            } else {
                                // Same-day service
                                arrival_seconds >= current_seconds
                            };
                            
                            if is_future {
                                // Get line info
                                let line_color = gtfs_cache.routes.get(&trip.route_id)
                                    .cloned()
                                    .unwrap_or_else(|| "808080".to_string());
                                
                                // Extract line code from route_id
                                let line_code = Self::extract_line_code_from_route(&trip.route_id, operator);
                                
                                scheduled_arrivals.push(ScheduledArrival {
                                    trip_id: stop_time.trip_id.clone(),
                                    route_id: trip.route_id.clone(),
                                    line_code,
                                    line_color,
                                    arrival_time: stop_time.arrival_time.clone(),
                                    departure_time: stop_time.departure_time.clone(),
                                    destination: trip.trip_headsign.clone(),
                                    stop_headsign: stop_time.stop_headsign.clone(),
                                    operator: operator.to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
        
        // Sort by arrival time
        scheduled_arrivals.sort_by(|a, b| a.arrival_time.cmp(&b.arrival_time));
        
        // Deduplicate based on line_code, arrival_time, and destination
        // Keep only the first occurrence of each unique combination
        let mut seen = std::collections::HashSet::new();
        scheduled_arrivals.retain(|arrival| {
            let key = (
                arrival.line_code.clone(),
                arrival.arrival_time.clone(),
                arrival.destination.clone().unwrap_or_default()
            );
            seen.insert(key)
        });
        
        // Take top results after deduplication
        scheduled_arrivals.truncate(max_results);
        scheduled_arrivals
    }
    
    /// Check if a service is active on a given date
    fn is_service_active(
        service_id: &str,
        date: &str,
        weekday: u32,
        calendar: &HashMap<String, ServiceCalendar>,
        calendar_dates: &HashMap<String, Vec<CalendarDate>>,
    ) -> bool {
        // First check calendar_dates for exceptions
        if let Some(exceptions) = calendar_dates.get(service_id) {
            for exception in exceptions {
                if exception.date == date {
                    // exception_type: 1 = service added, 2 = service removed
                    return exception.exception_type == 1;
                }
            }
        }
        
        // Check regular calendar
        if let Some(cal) = calendar.get(service_id) {
            // Check if date is within service period
            if date < cal.start_date.as_str() || date > cal.end_date.as_str() {
                return false;
            }
            
            // Check day of week
            return match weekday {
                0 => cal.monday,
                1 => cal.tuesday,
                2 => cal.wednesday,
                3 => cal.thursday,
                4 => cal.friday,
                5 => cal.saturday,
                6 => cal.sunday,
                _ => false,
            };
        }
        
        false
    }
    
    /// Parse GTFS time format (HH:MM:SS) to seconds since midnight
    fn parse_gtfs_time(time_str: &str) -> Option<u32> {
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() != 3 {
            return None;
        }
        
        let hours: u32 = parts[0].parse().ok()?;
        let minutes: u32 = parts[1].parse().ok()?;
        let seconds: u32 = parts[2].parse().ok()?;
        
        Some(hours * 3600 + minutes * 60 + seconds)
    }
    
    /// Extract line code from route ID for display
    fn extract_line_code_from_route(route_id: &str, operator: &str) -> String {
        if operator == "TBM" {
            // TBM format: extract last part
            route_id.split(':').last().unwrap_or(route_id).to_string()
        } else if operator == "TransGironde" {
            // TransGironde format: GIRONDE:Line:XXXX -> XXXX
            route_id.split(':').last().unwrap_or(route_id).to_string()
        } else {
            // SNCF and others: use as is
            route_id.to_string()
        }
    }

    /// Get detailed information about a specific vehicle including stop sequence
    pub fn get_vehicle_details(vehicle_id: &str, cache: &CachedNetworkData) -> Option<VehicleDetails> {
        // Find the vehicle in real-time data
        let vehicle = cache.real_time.iter().find(|v| v.vehicle_id == vehicle_id)?;

        // Find the line this vehicle belongs to
        let network_data = cache.to_network_data();
        let line = network_data.lines.iter().find(|l| {
            l.real_time.iter().any(|rt| rt.vehicle_id == vehicle_id)
        })?;

        // Get the trip information to find stop sequence
        let gtfs_caches = vec![
            (&cache.tbm_gtfs_cache, "TBM"),
            (&cache.transgironde_gtfs_cache, "TransGironde"),
            (&cache.sncf_gtfs_cache, "SNCF"),
        ];

        let mut current_stop = None;
        let mut next_stop = None;
        let mut previous_stop = None;

        // Find stop sequence from trip information
        for (gtfs_cache, _operator) in gtfs_caches {
            if let Some(_trip) = gtfs_cache.trips.get(&vehicle.trip_id) {
                // Get all stops for this trip in sequence
                let mut trip_stops: Vec<_> = gtfs_cache.stop_times.values()
                    .flatten()
                    .filter(|st| st.trip_id == vehicle.trip_id)
                    .collect();
                
                trip_stops.sort_by_key(|st| st.stop_sequence);

                // Try to find current stop position using current_stop_sequence first (most accurate)
                let current_idx = if let Some(seq) = vehicle.current_stop_sequence {
                    // Use the sequence number from GTFS-RT to find exact position
                    trip_stops.iter().position(|st| st.stop_sequence == seq)
                } else if let Some(current_stop_id) = &vehicle.stop_id {
                    // Fallback: find by stop_id (may not work correctly for duplicate stops)
                    trip_stops.iter().position(|st| &st.stop_id == current_stop_id)
                } else {
                    None
                };

                if let Some(idx) = current_idx {
                    // Get current stop
                    if let Some(current_stop_id) = vehicle.stop_id.as_ref().or_else(|| {
                        trip_stops.get(idx).map(|st| &st.stop_id)
                    }) {
                        current_stop = network_data.stops.iter()
                            .find(|s| &s.stop_id == current_stop_id)
                            .cloned();
                    }

                    // Get next stop
                    if idx + 1 < trip_stops.len() {
                        let next_stop_id = &trip_stops[idx + 1].stop_id;
                        next_stop = network_data.stops.iter()
                            .find(|s| &s.stop_id == next_stop_id)
                            .cloned();
                    }

                    // Get previous stop
                    if idx > 0 {
                        let prev_stop_id = &trip_stops[idx - 1].stop_id;
                        previous_stop = network_data.stops.iter()
                            .find(|s| &s.stop_id == prev_stop_id)
                            .cloned();
                    }
                }
                break;
            }
        }

        Some(VehicleDetails {
            vehicle_id: vehicle.vehicle_id.clone(),
            trip_id: vehicle.trip_id.clone(),
            route_id: vehicle.route_id.clone(),
            line_code: line.line_code.clone(),
            line_name: line.line_name.clone(),
            line_color: line.color.clone(),
            operator: line.operator.clone(),
            destination: vehicle.destination.clone(),
            current_stop,
            next_stop,
            previous_stop,
            latitude: vehicle.latitude,
            longitude: vehicle.longitude,
            timestamp: vehicle.timestamp,
            delay: vehicle.delay,
        })
    }
}