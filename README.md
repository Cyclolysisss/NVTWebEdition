# NVT Web Edition - New-Aquitaine Transit Network Viewer

A real-time transit network visualization and API server for New-Aquitaine region networks including TBM (Bordeaux Métropole), SNCF trains, and all regional transit operators across New-Aquitaine. This application provides a comprehensive web interface for tracking vehicles, viewing routes, planning trips, and accessing transit data through a RESTful API.

## 🌟 Features

### Real-Time Tracking
- **Live Vehicle Positions**: Track buses, trams, and regional vehicles in real-time
- **Vehicle Tracking**: Select and track individual vehicles with detailed stop information
- **Auto-Refresh**: Network data automatically updates every 30 seconds
- **Vehicle Information**: View detailed information including destination, delay, terminus, current stop, next stop, and previous stop
- **SNCF Integration**: Real-time trip updates and service alerts from SNCF trains

### Interactive Map
- **Multi-Operator Support**: Integrated display of TBM, all New-Aquitaine regional networks, and SNCF trains
- **Line Shapes**: Visualize complete route geometries on the map
- **Stop Information**: View all stops with real-time arrival predictions
- **Alerts & Disruptions**: See active service alerts and disruptions
- **3D Buildings**: Enhanced visualization with 3D building rendering
- **Heatmap Mode**: Visualize vehicle density across the network

### Trip Planning
- **Transit Route Planner**: Calculate optimal routes between any two stops
- **Multi-Operator Routing**: Routes can use lines from TBM, regional networks, and SNCF
- **Transfer Information**: Clear display of required transfers with minimum transfer times
- **Real-Time Arrival Predictions**: See next vehicle arrivals at each stop

### Network Organization
- **Line Grouping**: Lines organized by type (Tram, BRT, Bus, Regional, School, Night, Ferry)
- **Search & Filter**: Quickly find stops and lines
- **Operator Badges**: Clear identification of services by operator (TBM, Calibus, YELO, Tanlib, STCLM, etc.)

### RESTful API
- **Comprehensive Endpoints**: Access all network data programmatically
- **JSON Responses**: Standard JSON format with metadata
- **CORS Enabled**: Cross-origin requests supported
- **Health Check**: Monitor server status

## 📋 Table of Contents

- [Prerequisites](#prerequisites)
- [Installation](#installation)
- [Building from Source](#building-from-source)
- [Running the Server](#running-the-server)
- [Usage](#usage)
- [API Documentation](#api-documentation)
- [Configuration](#configuration)
- [Data Sources](#data-sources)
- [Architecture](#architecture)
- [Development](#development)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)

## 🔧 Prerequisites

Before running this application, ensure you have the following installed:

- **Rust** (1.70 or later) - [Install from rustup.rs](https://rustup.rs/)
- **Cargo** - Comes with Rust installation
- **Internet Connection** - Required for fetching real-time transit data

## 📦 Installation

### Using Pre-built Binary

1. Download the latest release from the releases page
2. Extract the binary
3. Run the executable

### Building from Source

1. Clone the repository:
```bash
git clone https://github.com/Cyclolysisss/NVTWebEdition.git
cd NVTWebEdition
```

2. Build the project:
```bash
cargo build --release
```

The compiled binary will be located at `target/release/nvt-web-edition` (or `.exe` on Windows).

## 🚀 Running the Server

### Quick Start

Simply run the binary:
```bash
cargo run --release
```

Or if using a pre-built binary:
```bash
./nvt-web-edition
```

### First Launch

On first launch, the server will:
1. Download GTFS data for TBM, New-Aquitaine regional networks, and SNCF
2. Cache the data locally (expires after 15-30 days depending on operator)
3. Fetch real-time vehicle positions and alerts
4. Start the web server on port 8080

This initial setup may take 30-60 seconds depending on network speed.

### Accessing the Application

Once running, access the application at:
- **Web UI**: http://localhost:8080
- **API Base**: http://localhost:8080/api/tbm
- **Health Check**: http://localhost:8080/health

## 💻 Usage

### Web Interface

#### Viewing the Network

1. Open http://localhost:8080 in your web browser
2. The map displays all stops, vehicles, and line routes
3. Use the legend on the right to browse lines by category
4. Click on any line to see its route and stops
5. Click on vehicles or stops for detailed information

#### Planning a Trip

1. Click the "🗺️ Plan Trip" button in the controls panel
2. Click on a stop to set it as your starting point
3. Click on another stop to set it as your destination
4. The system will calculate the best route with:
    - Estimated duration
    - Number of transfers required
    - Next arrival times at each stop
    - Step-by-step directions

#### Filtering the Display

Use the checkboxes in the controls panel to show/hide:
- **Line Routes** - Route geometries on the map
- **Vehicles** - Real-time vehicle positions
- **Stops** - All transit stops
- **Alerts** - Service disruptions
- **Heatmap** - Vehicle density visualization

#### Searching

Use the search boxes to quickly find:
- Stops by name or ID
- Lines by code or name
- The map will automatically zoom to your search result

### API Usage

#### Get All Network Data

```bash
curl http://localhost:8080/api/tbm/network
```

Response includes stops, lines, and shapes for both operators.

#### Get All Stops

```bash
curl http://localhost:8080/api/tbm/stops
```

#### Get All Lines

```bash
curl http://localhost:8080/api/tbm/lines
```

#### Get Real-Time Vehicle Positions

```bash
curl http://localhost:8080/api/tbm/vehicles
```

#### Get Active Alerts

```bash
curl http://localhost:8080/api/tbm/alerts
```

#### Get Specific Stop

```bash
curl http://localhost:8080/api/tbm/stop/{stop_id}
```

#### Get Stop Schedule

```bash
curl http://localhost:8080/api/tbm/stop/{stop_id}/schedule
```

Returns scheduled arrivals for the stop with deduplication.

#### Get Vehicle Details

```bash
curl http://localhost:8080/api/tbm/vehicle/{vehicle_id}
```

Returns detailed information about a specific vehicle including its current terminus, current stop, next stop, and previous stop.

#### Get Specific Line

```bash
curl http://localhost:8080/api/tbm/line/{line_code}
```

Example:
```bash
curl http://localhost:8080/api/tbm/line/A
```

#### Get Lines by Operator

```bash
curl http://localhost:8080/api/tbm/operator/{operator_name}
```

Examples:
```bash
curl http://localhost:8080/api/tbm/operator/TBM
curl "http://localhost:8080/api/tbm/operator/YELO"
curl "http://localhost:8080/api/tbm/operator/Calibus (Libourne)"
curl "http://localhost:8080/api/tbm/operator/STCLM (Limoges Métropole)"
```

#### Get All Operators

```bash
curl http://localhost:8080/api/tbm/operators
```

#### Get Cache Statistics

```bash
curl http://localhost:8080/api/tbm/stats
```

#### Force Data Refresh

```bash
curl -X POST http://localhost:8080/api/tbm/refresh
```

#### Health Check

```bash
curl http://localhost:8080/health
```

### API Response Format

All API responses follow this format:

```json
{
  "success": true,
  "data": { /* response data */ },
  "error": null,
  "timestamp": 1700123456,
  "sources": ["TBM", "NewAquitaine", "SNCF"]
}
```

Error responses:

```json
{
  "success": false,
  "data": null,
  "error": "Error message here",
  "timestamp": 1700123456,
  "sources": []
}
```

## ⚙️ Configuration

### Server Port

The server runs on port 8080 by default. To change this, modify the `bind` address in `src/main.rs`:

```rust
.bind(("0.0.0.0", 8080))?
```

### Auto-Refresh Interval

Real-time data refreshes every 30 seconds. To change this, modify the interval in `src/main.rs`:

```rust
let mut interval = time::interval(Duration::from_secs(30));
```

### Cache Expiration

- **TBM GTFS Data**: Expires after 15 days
- **New-Aquitaine GTFS Data**: Expires after 30 days
- **SNCF GTFS Data**: Expires after 30 days

To modify, change the values in `src/tbm_api_models.rs`:

```rust
if let Some(cache) = GTFSCache::load("TBM", 15) {  // days
if let Some(cache) = GTFSCache::load("NewAquitaine", 30) {  // days
if let Some(cache) = GTFSCache::load("SNCF", 30) {  // days
```

### Mapbox Token

The web interface uses a Mapbox token for the map display. To use your own token, replace it in `static/tbm-transit.js`:

```javascript
this.mapboxToken = 'YOUR_TOKEN_HERE';
```

Get a free token at [mapbox.com](https://www.mapbox.com/).

## 📊 Data Sources

### TBM (Transports Bordeaux Métropole)

**Official Website**: https://www.infotbm.com/

**API Endpoints**:
- Stop Discovery (SIRI-Lite): https://bdx.mecatran.com/utw/ws/siri/2.0/bordeaux/stoppoints-discovery.json
- Lines Discovery (SIRI-Lite): https://bdx.mecatran.com/utw/ws/siri/2.0/bordeaux/lines-discovery.json
- GTFS-RT Vehicles: https://bdx.mecatran.com/utw/ws/gtfsfeed/vehicles/bordeaux
- GTFS-RT Alerts: https://bdx.mecatran.com/utw/ws/gtfsfeed/alerts/bordeaux
- GTFS-RT Trip Updates: https://bdx.mecatran.com/utw/ws/gtfsfeed/realtime/bordeaux
- GTFS Static: https://transport.data.gouv.fr/resources/83024/download

### New-Aquitaine Regional Networks

**Official Website**: https://www.nouvelle-aquitaine.fr/

**Data Source**:
- GTFS Static (Aggregated): https://www.pigma.org/public/opendata/nouvelle_aquitaine_mobilites/publication/naq-aggregated-gtfs.zip

**Included Operators** (50+ transit networks):
- **TBM** (Bordeaux Métropole)
- **Calibus** (Libourne)
- **YELO** (La Rochelle)
- **Tanlib** (Niort)
- **STCLM** (Limoges Métropole)
- **VITALIS** (Grand Poitiers)
- **Evalys** (Val de Garonne)
- **Périmouv** (Grand Perigueux)
- **IDELIS** (Pau)
- **TMA** (Mont de Marsan)
- **Mobius** (Grand Angoulême)
- **BUSS** (Saintes)
- **Carabus** (Royan Atlantique)
- And 40+ more regional and departmental transit operators across New-Aquitaine

### SNCF (French National Railways)

**Official Website**: https://www.sncf.com/

**Data Sources**:
- GTFS Static: https://eu.ftp.opendatasoft.com/sncf/plandata/Export_OpenData_SNCF_GTFS_NewTripId.zip
- GTFS-RT Trip Updates: https://proxy.transport.data.gouv.fr/resource/sncf-gtfs-rt-trip-updates
- GTFS-RT Service Alerts: https://proxy.transport.data.gouv.fr/resource/sncf-gtfs-rt-service-alerts

## 🏗️ Architecture

### Technology Stack

- **Backend**: Rust with Actix-Web framework
- **Data Formats**: GTFS, GTFS-RT, SIRI-Lite
- **Frontend**: Vanilla JavaScript with Mapbox GL JS
- **Data Storage**: In-memory cache with filesystem persistence

### Components

#### Backend (`src/main.rs`)
- HTTP server using Actix-Web
- Embedded static files (HTML, JavaScript)
- API route handlers
- Background auto-refresh task
- CORS middleware for cross-origin requests

#### API Models (`src/tbm_api_models.rs`)
- GTFS and GTFS-RT data parsing
- Network data caching system
- Multi-operator data integration (TBM, New-Aquitaine networks, SNCF)
- Agency-based operator identification
- Real-time feed processing
- Shape geometry handling
- Transfer rules support

#### Frontend (`static/nvtweb.html`)
- Responsive HTML interface
- Modern CSS with gradients and animations
- Mobile-friendly design

#### Map Application (`static/tbm-transit.js`)
- Mapbox GL JS integration
- Real-time vehicle tracking
- Transit route calculation
- Stop and line search
- Interactive popup displays

### Data Flow

1. **Initialization**:
    - Server downloads/loads cached GTFS data
    - Processes routes, stops, and shapes
    - Fetches initial real-time data

2. **Auto-Refresh** (every 30 seconds):
    - Fetches vehicle positions
    - Updates service alerts
    - Retrieves trip updates
    - Static data refreshed every hour if needed

3. **Client Requests**:
    - Frontend requests data from API
    - Server returns cached/real-time data
    - Client updates map visualization

### Caching Strategy

**GTFS Static Data**:
- Stored in `~/.cache/tbm_nvt/` (Linux/Mac) or equivalent
- Files: `tbm_gtfs_cache.json`, `newaquitaine_gtfs_cache.json`, `sncf_gtfs_cache.json`
- Contains: routes, stops, shapes, route-to-shape mappings, agencies, transfers
- Automatically refreshed when expired

**Real-Time Data**:
- Stored in memory
- Updated every 30 seconds
- Includes: vehicle positions, alerts, trip updates

## 🛠️ Development

### Project Structure

```
NVTWebEdition/
├── src/
│   ├── main.rs              # Main server and API routes
│   └── tbm_api_models.rs    # Data models and fetching logic
├── static/
│   ├── nvtweb.html          # Frontend HTML
│   └── tbm-transit.js       # Frontend JavaScript application
├── Cargo.toml               # Rust dependencies (not in repo yet)
└── README.md                # This file
```

### Dependencies

Create a `Cargo.toml` file with these dependencies:

```toml
[package]
name = "nvt-web-edition"
version = "1.1.0"
edition = "2021"

[dependencies]
actix-web = "4"
actix-cors = "0.7"
actix-files = "0.6"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.11", features = ["blocking"] }
gtfs-rt = "0.3"
prost = "0.12"
chrono = "0.4"
chrono-tz = "0.8"
csv = "1.2"
zip = "0.6"
bytes = "1.4"
dirs = "5.0"
```

### Building for Production

```bash
cargo build --release --target x86_64-unknown-linux-gnu
```

Or for other targets:
```bash
cargo build --release --target x86_64-pc-windows-gnu
cargo build --release --target x86_64-apple-darwin
```

### Running in Development Mode

```bash
cargo run
```

This will build and run with debug symbols and optimizations disabled for faster compilation.

### Testing

Currently, there are no automated tests. To test manually:

1. Start the server
2. Open http://localhost:8080
3. Verify map loads with stops and lines
4. Test API endpoints with curl
5. Try planning a route between two stops
6. Check console logs for errors

### Code Style

This project follows standard Rust conventions:
- Use `rustfmt` for code formatting
- Use `clippy` for linting

```bash
cargo fmt
cargo clippy
```

## 🔍 Troubleshooting

### Server Won't Start

**Problem**: Server fails to initialize
**Solutions**:
1. Check internet connection (required for initial data download)
2. Verify no other service is using port 8080
3. Check console output for specific errors
4. Try manually refreshing: `curl -X POST http://localhost:8080/api/tbm/refresh`

### No Vehicles Showing

**Problem**: Map displays stops but no vehicles
**Solutions**:
1. Ensure "Vehicles" checkbox is enabled
2. Check if vehicles are operating (time of day, day of week)
3. Wait for next auto-refresh (30 seconds)
4. Force refresh in the web interface

### Map Not Loading

**Problem**: Blank map or loading spinner
**Solutions**:
1. Check browser console for JavaScript errors
2. Verify Mapbox token is valid
3. Check internet connection
4. Try different browser
5. Clear browser cache

### Cache Issues

**Problem**: Old or incorrect data displaying
**Solutions**:
1. Delete cache files:
    - Linux/Mac: `~/.cache/tbm_nvt/`
    - Windows: `%APPDATA%\Local\tbm_nvt\`
2. Restart server to download fresh data
3. Use force refresh API endpoint

### CORS Errors

**Problem**: API requests fail with CORS errors
**Solution**: The server uses permissive CORS. If issues persist:
1. Check if server is running
2. Verify request URL is correct
3. Check browser console for specific CORS error

### Memory Usage

**Problem**: High memory consumption
**Solutions**:
1. This is expected with full network data in memory
2. Typical usage: 100-300 MB
3. Memory is stable (no leaks)
4. Restart server if needed

## 🤝 Contributing

Contributions are welcome! Here's how to contribute:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Test thoroughly
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to your branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

### Areas for Contribution

- **Testing**: Add unit and integration tests
- **Documentation**: Improve code comments and documentation
- **Features**: Add new API endpoints or map features
- **Performance**: Optimize data fetching and rendering
- **UI/UX**: Improve the web interface design
- **Accessibility**: Enhance accessibility features
- **Internationalization**: Add support for multiple languages
- **Mobile**: Improve mobile responsiveness

## 📝 License

This project is open source. Please check the repository for license information.

## 👤 Author

**Cyclolysisss**

Project Link: https://github.com/Cyclolysisss/NVTWebEdition

## 🙏 Acknowledgments

- **TBM (Transports Bordeaux Métropole)** for providing open transit data
- **Région Nouvelle-Aquitaine** for aggregated regional transit data covering 50+ operators
- **SNCF** for national railway data and real-time updates
- **Mapbox** for mapping services
- **Actix-Web** for the robust web framework
- The Rust community for excellent tools and libraries

## 📅 Version History

### Version 1.4.0 (Current)
- **New-Aquitaine Integration**: Replaced regional GTFS with comprehensive aggregated feed
- **50+ Transit Operators**: Now supporting all New-Aquitaine transit networks
- **Agency-Based Identification**: Automatic operator detection from GTFS agency data
- **Transfer Rules Support**: Added support for GTFS transfers.txt for better trip planning
- **Enhanced Data Model**: Extended cache structure to include agencies and route-to-agency mappings

### Version 1.3.0
- **Vehicle Tracking Menu**: Track individual vehicles with detailed stop information
- **SNCF Integration**: Added SNCF GTFS-RT trip updates and service alerts
- **Improved Schedules**: Deduplication logic prevents duplicate arrivals
- **New API Endpoints**: `/api/tbm/vehicle/{id}` and `/api/tbm/stop/{id}/schedule`
- **Code Optimizations**: HTTP client helper, efficient deduplication with tuples

### Version 1.2.0
- Added SNCF static data support (routes, stops, shapes)
- Enhanced line type classification for trains

### Version 1.1.0
- Integrated TransGironde regional transit network
- Added dual-operator support throughout the application
- Implemented transit route planner with multi-operator routing
- Enhanced UI with operator badges and better organization
- Improved caching system with operator-specific expiration

### Version 1.0.0
- Initial release with TBM support
- Real-time vehicle tracking
- Interactive map interface
- RESTful API
- GTFS and GTFS-RT integration

## 🔮 Future Plans

- [ ] User accounts and saved routes
- [ ] Timetable display for stops
- [ ] Fare calculator
- [ ] Accessibility information
- [ ] Historical data and analytics
- [ ] Mobile app (React Native or Flutter)
- [ ] Additional transit operators
- [ ] Real-time notifications
- [ ] Offline mode support
- [ ] Docker containerization

## 📞 Support

For issues, questions, or suggestions:
- Open an issue on GitHub
- Check existing issues for solutions
- Refer to the troubleshooting section

---

**Built with ❤️ in Rust** | **Transit Data Powered by TBM, Région Nouvelle-Aquitaine & SNCF**
