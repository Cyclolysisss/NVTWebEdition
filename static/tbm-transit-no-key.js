// TBM + TransGironde Transit Network Visualization - OPTIMIZED VERSION
// Enhanced with dual-operator support and advanced routing
// Version: 1.2.0
// Last Updated: 2025-11-20 06:38:04 UTC
// User: Cyclolysisss

class TBMTransitMap {
    constructor() {
        this.map = null;
        this.networkData = null;
        this.selectedLine = null;
        this.selectedLineStops = [];
        this.collapsedGroups = new Set();
        this.updateInterval = null;
        this.directionsVisible = false;
        this.routeStart = null;
        this.routeEnd = null;
        this.transitRoutes = [];

        // Performance optimizations
        this.searchDebounceTimer = null;
        this.renderDebounceTimer = null;
        this.cachedStopGraph = null;
        this.cachedLinesByType = null;

        // Popup management
        this.currentPopup = null;

        // Prevent UI flashing
        this.isUpdating = false;
        this.updateQueue = [];

        // Enhanced line type classification with TransGironde and SNCF support
        this.lineTypes = {
            tram: {
                icon: '🚊',
                name: 'Tram',
                patterns: [/^[A-F]$/],
                order: 1
            },
            brt: {
                icon: '🚌',
                name: 'BRT (Lianes)',
                patterns: [/^[G-MO-RT-Z]$/, /^[G-MO-RT-Z]\d+$/],
                order: 2
            },
            bus: {
                icon: '🚍',
                name: 'Bus (TBM)',
                patterns: [/^([1-9]|[1-9]\d)$/, /^([1-9]|[1-9]\d)\s+(Est|Ouest|Nord|Sud)$/i, /^([1-9]|[1-9]\d)[EeOoNnSs]$/],
                order: 3
            },
            transgironde: {
                icon: '🚐',
                name: 'TransGironde Regional',
                patterns: [/^\d{3,4}$/],
                order: 4
            },
            tgv_inoui: {
                icon: '🚄',
                name: 'TGV INOUI',
                patterns: [/TGV\s*INOUI/i, /INOUI/i],
                order: 5
            },
            tgv: {
                icon: '🚅',
                name: 'TGV',
                patterns: [/^TGV$/i, /TGV(?!\s*INOUI)/i],
                order: 6
            },
            ice: {
                icon: '🚆',
                name: 'ICE (International)',
                patterns: [/ICE/i, /International/i],
                order: 7
            },
            ter: {
                icon: '🚂',
                name: 'Train TER',
                patterns: [/Train\s*TER/i, /^TER$/i],
                order: 8
            },
            ter_car: {
                icon: '🚌',
                name: 'Car TER',
                patterns: [/Car\s*TER/i],
                order: 9
            },
            navette: {
                icon: '🚐',
                name: 'Navette',
                patterns: [/Navette/i, /Shuttle/i],
                order: 10
            },
            school: {
                icon: '🎒',
                name: 'School Lines',
                patterns: [/^S\d+$/, /^S\s*\d+$/],
                order: 11
            },
            night: {
                icon: '🌙',
                name: 'Night Lines',
                patterns: [/^N\d+$/, /^N\s*\d+$/, /^TBNight$/],
                order: 12
            },
            ferry: {
                icon: '⛴️',
                name: 'BAT (Ferry)',
                patterns: [/^95[0-9]$/, /^BAT\s*\d*$/i, /^BAT$/i],
                order: 13
            },
            other: {
                icon: '🚐',
                name: 'Other',
                patterns: [],
                order: 99
            }
        };

        this.mapboxToken = 'pk.eyJ1IjoiY3ljbG9vbyIsImEiOiJjbWk0aGtxemEwd2R6MmxxdmRyN2g1Y3B0In0.t9ZLPDeBauN_Qwn9oo_s8Q';
        this.apiEndpoint = 'https://nvt.cyclooo.fr/api/tbm';

        this.init();
    }

    init() {
        mapboxgl.accessToken = this.mapboxToken;

        this.map = new mapboxgl.Map({
            container: 'map',
            style: this.getMapStyle(),
            center: [-0.5792, 44.8378],
            zoom: 11,
            pitch: 45,
            bearing: 0
        });

        window.map = this.map;

        this.map.addControl(new mapboxgl.NavigationControl());
        this.map.addControl(new mapboxgl.FullscreenControl());
        this.map.addControl(new mapboxgl.GeolocateControl({
            positionOptions: { enableHighAccuracy: true },
            trackUserLocation: true,
            showUserHeading: true
        }));

        this.map.on('load', () => {
            console.log('🗺️  Map loaded successfully');
            this.setupMapLayers();
            this.setupLineShapes();
            this.setupTransitRouteLayers();
            this.loadNetworkData();
            this.setupEventListeners();
            this.startAutoRefresh();
        });
    }

    getMapStyle() {
        const isDarkMode = document.documentElement.classList.contains('dark-theme');
        return isDarkMode ? 'mapbox://styles/mapbox/dark-v11' : 'mapbox://styles/mapbox/light-v11';
    }

    applyThemeToMap() {
        if (!this.map || !this.map.isStyleLoaded()) {
            console.log('⚠️ Map not ready for theme change');
            return;
        }
        
        const style = this.getMapStyle();
        
        console.log(`🎨 Applying theme to map: ${style}`);
        
        // Store current state
        const currentCenter = this.map.getCenter();
        const currentZoom = this.map.getZoom();
        const currentPitch = this.map.getPitch();
        const currentBearing = this.map.getBearing();
        
        this.map.setStyle(style);
        
        // Restore layers and state after style loads
        this.map.once('style.load', () => {
            console.log('🎨 Theme applied, restoring layers...');
            
            // Restore camera position
            this.map.jumpTo({
                center: currentCenter,
                zoom: currentZoom,
                pitch: currentPitch,
                bearing: currentBearing
            });
            
            // Restore all layers
            this.setupMapLayers();
            this.setupLineShapes();
            this.setupTransitRouteLayers();
            
            // Re-render data if available
            if (this.networkData) {
                requestAnimationFrame(() => {
                    this.updateMap();
                    this.updateLineShapes();
                    if (this.selectedLine) {
                        this.updateLineShapes(this.selectedLine);
                        this.updateSelectedLineStops(this.selectedLine);
                    }
                });
            }
        });
    }

    setupTransitRouteLayers() {
        this.map.addSource('transit-route', {
            type: 'geojson',
            data: { type: 'FeatureCollection', features: [] }
        });

        this.map.addLayer({
            id: 'transit-route-walking',
            type: 'line',
            source: 'transit-route',
            filter: ['==', ['get', 'type'], 'walking'],
            layout: { 'line-join': 'round', 'line-cap': 'round' },
            paint: {
                'line-color': '#666666',
                'line-width': 3,
                'line-dasharray': [2, 2],
                'line-opacity': 0.8
            }
        });

        this.map.addLayer({
            id: 'transit-route-transit',
            type: 'line',
            source: 'transit-route',
            filter: ['==', ['get', 'type'], 'transit'],
            layout: { 'line-join': 'round', 'line-cap': 'round' },
            paint: {
                'line-color': ['get', 'color'],
                'line-width': 6,
                'line-opacity': 0.9
            }
        });

        this.map.addSource('transit-route-markers', {
            type: 'geojson',
            data: { type: 'FeatureCollection', features: [] }
        });

        this.map.addLayer({
            id: 'transit-route-markers-layer',
            type: 'circle',
            source: 'transit-route-markers',
            paint: {
                'circle-radius': 10,
                'circle-color': ['get', 'color'],
                'circle-stroke-width': 3,
                'circle-stroke-color': '#ffffff'
            }
        });

        this.map.addLayer({
            id: 'transit-route-markers-labels',
            type: 'symbol',
            source: 'transit-route-markers',
            layout: {
                'text-field': ['get', 'label'],
                'text-font': ['Open Sans Bold', 'Arial Unicode MS Bold'],
                'text-size': 12,
                'text-offset': [0, 2],
                'text-anchor': 'top'
            },
            paint: {
                'text-color': '#000',
                'text-halo-color': '#fff',
                'text-halo-width': 2
            }
        });
    }

    async calculateTransitRoute(startStopId, endStopId) {
        console.log(`🗺️  Calculating route: ${startStopId} → ${endStopId}`);

        this.showLoading(true);

        await new Promise(resolve => setTimeout(resolve, 50));

        const startStop = this.networkData.stops.find(s => s.stop_id === startStopId);
        const endStop = this.networkData.stops.find(s => s.stop_id === endStopId);

        if (!startStop || !endStop) {
            console.error('❌ Start or end stop not found');
            this.showLoading(false);
            this.showNotification('⚠️ Invalid stops selected', 'error');
            return null;
        }

        const routes = this.findTransitRoutes(startStop, endStop);

        this.showLoading(false);

        if (routes.length === 0) {
            this.showNotification('⚠️ No transit route found', 'error');
            return null;
        }

        const bestRoute = routes.sort((a, b) => {
            if (a.transfers !== b.transfers) return a.transfers - b.transfers;
            if (a.duration !== b.duration) return a.duration - b.duration;
            return a.distance - b.distance;
        })[0];

        console.log('✅ Best route found:', bestRoute);
        this.displayTransitRoute(bestRoute);
        return bestRoute;
    }

    findTransitRoutes(startStop, endStop, maxTransfers = 2) {
        const routes = [];
        const stopGraph = this.getStopGraph();

        const queue = [{
            currentStop: startStop,
            path: [],
            linesUsed: [],
            transfers: 0,
            distance: 0,
            duration: 0
        }];

        const visited = new Set();
        visited.add(startStop.stop_id);

        const maxIterations = 20000; // Increased from 10000
        let iterations = 0;
        const searchRange = 25; // Increased from 15 for better coverage

        while (queue.length > 0 && routes.length < 5 && iterations < maxIterations) {
            iterations++;
            const current = queue.shift();

            if (current.currentStop.stop_id === endStop.stop_id) {
                routes.push(current);
                continue;
            }

            if (current.transfers > maxTransfers) continue;

            const linesAtStop = this.networkData.lines.filter(line =>
                current.currentStop.lines && current.currentStop.lines.includes(line.line_ref)
            );

            for (const line of linesAtStop) {
                const lineStops = this.getStopsOnLine(line.line_ref);
                const currentIndex = lineStops.findIndex(s => s.stop_id === current.currentStop.stop_id);

                if (currentIndex === -1) continue;

                const checkIndices = [];
                // Expanded search range for better route discovery
                for (let i = Math.max(0, currentIndex - searchRange); i < Math.min(lineStops.length, currentIndex + searchRange); i++) {
                    if (i !== currentIndex) checkIndices.push(i);
                }

                for (const i of checkIndices) {
                    const nextStop = lineStops[i];
                    const visitKey = `${nextStop.stop_id}-${line.line_ref}`;

                    if (visited.has(visitKey)) continue;
                    visited.add(visitKey);

                    const isTransfer = current.linesUsed.length > 0 &&
                        !current.linesUsed.some(l => l.line_ref === line.line_ref);

                    if (isTransfer && current.transfers >= maxTransfers) continue;

                    const segment = {
                        from: current.currentStop,
                        to: nextStop,
                        line: line,
                        stops: this.getStopsBetween(lineStops, currentIndex, i)
                    };

                    const segmentDistance = this.calculateDistance(
                        current.currentStop.latitude,
                        current.currentStop.longitude,
                        nextStop.latitude,
                        nextStop.longitude
                    );

                    const segmentDuration = this.estimateDuration(segment, line);

                    queue.push({
                        currentStop: nextStop,
                        path: [...current.path, segment],
                        linesUsed: isTransfer ? [...current.linesUsed, line] :
                            current.linesUsed.length === 0 ? [line] : current.linesUsed,
                        transfers: isTransfer ? current.transfers + 1 : current.transfers,
                        distance: current.distance + segmentDistance,
                        duration: current.duration + segmentDuration + (isTransfer ? 5 : 0)
                    });
                }
            }
        }

        console.log(`📊 Found ${routes.length} routes after ${iterations} iterations`);
        return routes;
    }

    getStopGraph() {
        if (this.cachedStopGraph) return this.cachedStopGraph;

        const graph = new Map();

        this.networkData.lines.forEach(line => {
            const stops = this.getStopsOnLine(line.line_ref);

            stops.forEach((stop, index) => {
                if (!graph.has(stop.stop_id)) {
                    graph.set(stop.stop_id, []);
                }

                if (index > 0) {
                    graph.get(stop.stop_id).push({
                        stop: stops[index - 1],
                        line: line
                    });
                }
                if (index < stops.length - 1) {
                    graph.get(stop.stop_id).push({
                        stop: stops[index + 1],
                        line: line
                    });
                }
            });
        });

        this.cachedStopGraph = graph;
        return graph;
    }

    getStopsOnLine(lineRef) {
        const stops = this.networkData.stops.filter(stop =>
            stop.lines && stop.lines.includes(lineRef)
        );
        
        // If no stops found, return empty array
        if (stops.length === 0) return [];
        
        // Try to sort by stop_sequence if available, otherwise maintain existing order
        // Note: stop_sequence might not be available in all datasets
        return stops.sort((a, b) => {
            const seqA = a.stop_sequence || 0;
            const seqB = b.stop_sequence || 0;
            
            // If both have no sequence, sort by stop_id to maintain consistent ordering
            if (seqA === 0 && seqB === 0) {
                return a.stop_id.localeCompare(b.stop_id);
            }
            
            return seqA - seqB;
        });
    }

    getStopsBetween(lineStops, fromIndex, toIndex) {
        const start = Math.min(fromIndex, toIndex);
        const end = Math.max(fromIndex, toIndex);
        return lineStops.slice(start, end + 1);
    }

    calculateDistance(lat1, lon1, lat2, lon2) {
        const R = 6371;
        const dLat = this.toRad(lat2 - lat1);
        const dLon = this.toRad(lon2 - lon1);
        const a = Math.sin(dLat / 2) * Math.sin(dLat / 2) +
            Math.cos(this.toRad(lat1)) * Math.cos(this.toRad(lat2)) *
            Math.sin(dLon / 2) * Math.sin(dLon / 2);
        const c = 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
        return R * c;
    }

    toRad(degrees) {
        return degrees * Math.PI / 180;
    }

    estimateDuration(segment, line) {
        const lineType = this.classifyLine(line);
        const distance = segment.stops.length - 1;

        const speeds = {
            tram: 2.5,
            brt: 2,
            bus: 3,
            transgironde: 4,
            ferry: 5,
            school: 3,
            night: 3,
            other: 3
        };

        return distance * (speeds[lineType] || 3);
    }

    displayTransitRoute(route) {
        const features = [];
        const markers = [];

        markers.push({
            type: 'Feature',
            geometry: {
                type: 'Point',
                coordinates: [route.path[0].from.longitude, route.path[0].from.latitude]
            },
            properties: {
                label: 'A',
                color: '#00AA00',
                type: 'start'
            }
        });

        route.path.forEach((segment, index) => {
            const line = segment.line;
            const color = `#${line.color || '808080'}`;

            const coordinates = segment.stops.map(stop => [stop.longitude, stop.latitude]);

            features.push({
                type: 'Feature',
                geometry: {
                    type: 'LineString',
                    coordinates: coordinates
                },
                properties: {
                    type: 'transit',
                    color: color,
                    line_code: line.line_code,
                    line_name: line.line_name,
                    operator: line.operator,
                    segment: index
                }
            });

            if (index < route.path.length - 1) {
                markers.push({
                    type: 'Feature',
                    geometry: {
                        type: 'Point',
                        coordinates: [segment.to.longitude, segment.to.latitude]
                    },
                    properties: {
                        label: '↔️',
                        color: '#FFA500',
                        type: 'transfer',
                        stop_name: segment.to.stop_name
                    }
                });
            }
        });

        const lastSegment = route.path[route.path.length - 1];
        markers.push({
            type: 'Feature',
            geometry: {
                type: 'Point',
                coordinates: [lastSegment.to.longitude, lastSegment.to.latitude]
            },
            properties: {
                label: 'B',
                color: '#DD0000',
                type: 'end'
            }
        });

        this.map.getSource('transit-route').setData({
            type: 'FeatureCollection',
            features: features
        });

        this.map.getSource('transit-route-markers').setData({
            type: 'FeatureCollection',
            features: markers
        });

        const allCoordinates = features.flatMap(f => f.geometry.coordinates);
        if (allCoordinates.length > 0) {
            const bounds = allCoordinates.reduce((bounds, coord) => {
                return bounds.extend(coord);
            }, new mapboxgl.LngLatBounds(allCoordinates[0], allCoordinates[0]));

            this.map.fitBounds(bounds, {
                padding: 80,
                duration: 2000
            });
        }

        this.showTransitRouteDetails(route);
    }

    showTransitRouteDetails(route) {
        const panel = document.getElementById('directionsPanel');
        const content = document.getElementById('directionsContent');

        const totalDuration = Math.round(route.duration);
        const totalDistance = route.distance.toFixed(2);
        const transfers = route.transfers;

        let stepsHTML = '<div class="route-steps">';

        route.path.forEach((segment, index) => {
            const line = segment.line;
            const lineType = this.lineTypes[this.classifyLine(line)];
            const stops = segment.stops.length - 1;
            const duration = Math.round(this.estimateDuration(segment, line));

            const operatorBadge = this.getOperatorBadge(line.operator);

            let nextArrival = '';
            if (segment.from.real_time && segment.from.real_time.length > 0) {
                const arrivals = segment.from.real_time.filter(rt =>
                    rt.route_id === line.route_id
                ).sort((a, b) => (a.timestamp || 0) - (b.timestamp || 0));

                if (arrivals.length > 0 && arrivals[0].timestamp) {
                    const time = new Date(arrivals[0].timestamp * 1000);
                    const now = new Date();
                    const minutesUntil = Math.floor((time - now) / 60000);
                    if (minutesUntil >= 0) {
                        nextArrival = minutesUntil === 0 ? 'Now' : `${minutesUntil} min`;
                    }
                }
            }

            stepsHTML += `
                <div class="route-step">
                    <div class="route-step-header">
                        <div class="route-step-icon">${lineType.icon}</div>
                        <div class="route-step-info">
                            <div class="route-step-line">
                                <span class="line-badge" style="background-color: #${line.color}; color: white;">
                                    ${line.line_code}
                                </span>
                                <span class="route-step-line-name">${line.line_name}</span>
                                ${operatorBadge}
                            </div>
                            <div class="route-step-meta">
                                ${stops} stop${stops !== 1 ? 's' : ''} • ${duration} min
                                ${nextArrival ? `<span class="next-arrival"> • Next: ${nextArrival}</span>` : ''}
                            </div>
                        </div>
                    </div>
                    <div class="route-step-details">
                        <div class="route-step-stop">
                            <span class="route-step-bullet start">●</span>
                            <strong>${segment.from.stop_name}</strong>
                        </div>
                        ${stops > 1 ? `<div class="route-step-stop intermediate">↓ ${stops - 1} intermediate stop${stops - 1 !== 1 ? 's' : ''}</div>` : ''}
                        <div class="route-step-stop">
                            <span class="route-step-bullet end">●</span>
                            <strong>${segment.to.stop_name}</strong>
                        </div>
                    </div>
                </div>
            `;

            if (index < route.path.length - 1) {
                stepsHTML += `
                    <div class="route-transfer">
                        <div class="route-transfer-icon">↔️</div>
                        <div class="route-transfer-text">Transfer at ${segment.to.stop_name}</div>
                    </div>
                `;
            }
        });

        stepsHTML += '</div>';

        content.innerHTML = `
            <div class="route-summary-transit">
                <div class="route-summary-header">
                    <h4>🚍 Transit Route Found</h4>
                    <button class="close-route-btn" onclick="tbmMap.clearTransitRoute()">✕</button>
                </div>
                <div class="route-summary-stats">
                    <div class="route-stat">
                        <div class="route-stat-value">${totalDuration}</div>
                        <div class="route-stat-label">minutes</div>
                    </div>
                    <div class="route-stat">
                        <div class="route-stat-value">${transfers}</div>
                        <div class="route-stat-label">transfer${transfers !== 1 ? 's' : ''}</div>
                    </div>
                    <div class="route-stat">
                        <div class="route-stat-value">${totalDistance}</div>
                        <div class="route-stat-label">km</div>
                    </div>
                </div>
            </div>
            ${stepsHTML}
        `;

        panel.style.display = 'block';
        this.directionsVisible = true;
    }

    clearTransitRoute() {
        this.map.getSource('transit-route').setData({
            type: 'FeatureCollection',
            features: []
        });

        this.map.getSource('transit-route-markers').setData({
            type: 'FeatureCollection',
            features: []
        });

        this.routeStart = null;
        this.routeEnd = null;
        this.transitRoutes = [];

        const panel = document.getElementById('directionsPanel');
        const content = document.getElementById('directionsContent');

        content.innerHTML = `
            <p style="font-size: 13px; color: var(--text-tertiary); margin-bottom: 12px; line-height: 1.6;">
                Click on any two stops on the map to plan your transit route. 
                The system will calculate the best route using TBM and TransGironde lines.
            </p>
        `;

        this.showNotification('✓ Route cleared');
    }

    setStopAsDirectionPoint(stopId, isStart = null) {
        const stop = this.networkData.stops.find(s => s.stop_id === stopId);
        if (!stop) return;

        if (isStart === null) {
            if (!this.routeStart) {
                isStart = true;
            } else if (!this.routeEnd) {
                isStart = false;
            } else {
                this.clearTransitRoute();
                isStart = true;
            }
        }

        if (isStart) {
            this.routeStart = stop;
            this.showNotification(`📍 Start: ${stop.stop_name}`);

            this.map.getSource('transit-route-markers').setData({
                type: 'FeatureCollection',
                features: [{
                    type: 'Feature',
                    geometry: {
                        type: 'Point',
                        coordinates: [stop.longitude, stop.latitude]
                    },
                    properties: {
                        label: 'A',
                        color: '#00AA00',
                        type: 'start'
                    }
                }]
            });
        } else {
            this.routeEnd = stop;
            this.showNotification(`🎯 Destination: ${stop.stop_name}`);

            if (this.routeStart && this.routeEnd) {
                this.calculateTransitRoute(this.routeStart.stop_id, this.routeEnd.stop_id);
            }
        }
    }

    showNotification(message, type = 'success') {
        const indicator = document.getElementById('updateIndicator');
        indicator.textContent = message;
        indicator.style.background = type === 'error' ?
            'linear-gradient(135deg, #dc3545 0%, #c82333 100%)' :
            'linear-gradient(135deg, #28a745 0%, #20c997 100%)';
        indicator.classList.add('show');
        setTimeout(() => {
            indicator.classList.remove('show');
        }, 3000);
    }

    showLoading(show) {
        const overlay = document.getElementById('loadingOverlay');
        if (show) {
            overlay.classList.add('active');
        } else {
            overlay.classList.remove('active');
        }
    }

    classifyLine(line) {
        if (line.operator === "TransGironde") {
            const code = line.line_code.trim();
            if (/^\d{3,4}$/.test(code)) {
                return 'transgironde';
            }
        }

        if (line.operator === "SNCF") {
            const code = line.line_code.trim();
            const name = line.line_name.trim();
            
            // Check for SNCF line types based on patterns
            const sncfTypes = ['tgv_inoui', 'tgv', 'ice', 'ter', 'ter_car', 'navette'];
            for (const type of sncfTypes) {
                const config = this.lineTypes[type];
                for (const pattern of config.patterns) {
                    if (pattern.test(code) || pattern.test(name)) {
                        return type;
                    }
                }
            }
            return 'other';
        }

        const code = line.line_code.trim();
        const name = line.line_name.trim();

        const sortedTypes = Object.entries(this.lineTypes)
            .filter(([type]) => type !== 'other' && type !== 'transgironde' && 
                    !['tgv_inoui', 'tgv', 'ice', 'ter', 'ter_car', 'navette'].includes(type))
            .sort((a, b) => a[1].order - b[1].order);

        for (const [type, config] of sortedTypes) {
            for (const pattern of config.patterns) {
                if (pattern.test(code) || pattern.test(name)) {
                    if (type === 'bus') {
                        if (/^[A-Z](\d+)?$/.test(code) && !/^[1-9]\d?/.test(code)) {
                            continue;
                        }
                    }
                    return type;
                }
            }
        }

        return 'other';
    }

    getOperatorBadge(operator, short = false) {
        if (operator === 'TransGironde') {
            return short ? 
                '<span class="operator-badge transgironde">TG</span>' :
                '<span class="operator-badge transgironde">TransGironde</span>';
        } else if (operator === 'SNCF') {
            return short ?
                '<span class="operator-badge sncf">SNCF</span>' :
                '<span class="operator-badge sncf">SNCF</span>';
        } else {
            return '<span class="operator-badge">TBM</span>';
        }
    }

    groupLinesByType() {
        if (this.cachedLinesByType && this.networkData) {
            return this.cachedLinesByType;
        }

        if (!this.networkData || !this.networkData.lines) {
            return {};
        }

        const groups = {};

        this.networkData.lines.forEach(line => {
            const type = this.classifyLine(line);
            if (!groups[type]) {
                groups[type] = [];
            }
            groups[type].push(line);
        });

        Object.keys(groups).forEach(type => {
            groups[type].sort((a, b) => {
                const codeA = a.line_code.trim();
                const codeB = b.line_code.trim();

                if (type === 'tram') {
                    return codeA.localeCompare(codeB);
                }

                if (type === 'brt') {
                    const letterA = codeA.match(/^[A-Z]/)?.[0] || '';
                    const letterB = codeB.match(/^[A-Z]/)?.[0] || '';
                    const numA = parseInt(codeA.match(/\d+/)?.[0] || '0');
                    const numB = parseInt(codeB.match(/\d+/)?.[0] || '0');

                    if (letterA !== letterB) {
                        return letterA.localeCompare(letterB);
                    }
                    return numA - numB;
                }

                if (type === 'bus') {
                    const matchA = codeA.match(/^(\d+)(\s*(Est|Ouest|Nord|Sud|[EONS]))?/i);
                    const matchB = codeB.match(/^(\d+)(\s*(Est|Ouest|Nord|Sud|[EONS]))?/i);

                    if (matchA && matchB) {
                        const numA = parseInt(matchA[1]);
                        const numB = parseInt(matchB[1]);

                        if (numA !== numB) {
                            return numA - numB;
                        }

                        const dirA = (matchA[3] || '').toLowerCase();
                        const dirB = (matchB[3] || '').toLowerCase();
                        return dirA.localeCompare(dirB);
                    }
                }

                if (type === 'transgironde') {
                    const numA = parseInt(codeA) || 0;
                    const numB = parseInt(codeB) || 0;
                    return numA - numB;
                }

                if (type === 'school' || type === 'night') {
                    const numA = parseInt(codeA.match(/\d+/)?.[0] || '0');
                    const numB = parseInt(codeB.match(/\d+/)?.[0] || '0');
                    return numA - numB;
                }

                if (type === 'ferry') {
                    const numA = parseInt(codeA.match(/\d+/)?.[0] || '0');
                    const numB = parseInt(codeB.match(/\d+/)?.[0] || '0');
                    return numA - numB;
                }

                return codeA.localeCompare(codeB, undefined, { numeric: true });
            });
        });

        this.cachedLinesByType = groups;
        return groups;
    }

    setupMapLayers() {
        console.log('🗺️  Setting up map layers...');

        const layers = this.map.getStyle().layers;
        const labelLayerId = layers.find(
            (layer) => layer.type === 'symbol' && layer.layout && layer.layout['text-field']
        )?.id;

        if (labelLayerId && !this.map.getLayer('3d-buildings')) {
            this.map.addLayer({
                'id': '3d-buildings',
                'source': 'composite',
                'source-layer': 'building',
                'filter': ['==', 'extrude', 'true'],
                'type': 'fill-extrusion',
                'minzoom': 15,
                'paint': {
                    'fill-extrusion-color': '#aaa',
                    'fill-extrusion-height': [
                        'interpolate', ['linear'], ['zoom'],
                        15, 0, 15.05, ['get', 'height']
                    ],
                    'fill-extrusion-base': [
                        'interpolate', ['linear'], ['zoom'],
                        15, 0, 15.05, ['get', 'min_height']
                    ],
                    'fill-extrusion-opacity': 0.6
                }
            }, labelLayerId);
        }

        if (!this.map.getSource('vehicles')) {
            this.map.addSource('vehicles', {
                type: 'geojson',
                data: { type: 'FeatureCollection', features: [] }
            });
        }

        if (!this.map.getSource('stops')) {
            this.map.addSource('stops', {
                type: 'geojson',
                data: { type: 'FeatureCollection', features: [] }
            });
        }

        if (!this.map.getSource('selected-line-stops')) {
            this.map.addSource('selected-line-stops', {
                type: 'geojson',
                data: { type: 'FeatureCollection', features: [] }
            });
        }

        if (!this.map.getLayer('vehicles-layer')) {
            this.map.addLayer({
                id: 'vehicles-layer',
                type: 'circle',
                source: 'vehicles',
                paint: {
                    'circle-radius': [
                        'interpolate',
                        ['linear'],
                        ['zoom'],
                        10, 6,
                        15, 12,
                        18, 20
                    ],
                    'circle-color': ['get', 'vehicleColor'],
                    'circle-stroke-width': 2,
                    'circle-stroke-color': '#ffffff',
                    'circle-opacity': 0.9
                }
            });
        }

        if (!this.map.getLayer('vehicles-labels')) {
            this.map.addLayer({
                id: 'vehicles-labels',
                type: 'symbol',
                source: 'vehicles',
                minzoom: 14,
                layout: {
                    'text-field': ['get', 'line_code'],
                    'text-font': ['Open Sans Bold', 'Arial Unicode MS Bold'],
                    'text-size': 10,
                    'text-offset': [0, 0],
                    'text-anchor': 'center'
                },
                paint: {
                    'text-color': '#ffffff',
                    'text-halo-color': '#000000',
                    'text-halo-width': 1
                }
            });
        }

        if (!this.map.getLayer('vehicles-heatmap')) {
            this.map.addLayer({
                id: 'vehicles-heatmap',
                type: 'heatmap',
                source: 'vehicles',
                layout: { visibility: 'none' },
                paint: {
                    'heatmap-weight': 1,
                    'heatmap-intensity': 1,
                    'heatmap-radius': 30,
                    'heatmap-opacity': 0.8,
                    'heatmap-color': [
                        'interpolate',
                        ['linear'],
                        ['heatmap-density'],
                        0, 'rgba(33,102,172,0)',
                        0.2, 'rgb(103,169,207)',
                        0.4, 'rgb(209,229,240)',
                        0.6, 'rgb(253,219,199)',
                        0.8, 'rgb(239,138,98)',
                        1, 'rgb(178,24,43)'
                    ]
                }
            });
        }

        if (!this.map.getLayer('stops-layer')) {
            this.map.addLayer({
                id: 'stops-layer',
                type: 'circle',
                source: 'stops',
                paint: {
                    'circle-radius': [
                        'interpolate', ['linear'], ['zoom'],
                        10, 3, 15, 8
                    ],
                    'circle-color': [
                        'case',
                        ['>', ['get', 'alerts'], 0],
                        '#ff4444', '#007cbf'
                    ],
                    'circle-stroke-width': 2,
                    'circle-stroke-color': '#ffffff',
                    'circle-opacity': 0.8
                }
            });
        }

        if (!this.map.getLayer('selected-line-stops-layer')) {
            this.map.addLayer({
                id: 'selected-line-stops-layer',
                type: 'circle',
                source: 'selected-line-stops',
                paint: {
                    'circle-radius': [
                        'interpolate', ['linear'], ['zoom'],
                        10, 5, 15, 12
                    ],
                    'circle-color': '#ff6b00',
                    'circle-stroke-width': 3,
                    'circle-stroke-color': '#ffffff',
                    'circle-opacity': 0.9
                }
            });
        }

        if (!this.map.getLayer('selected-line-stops-labels')) {
            this.map.addLayer({
                id: 'selected-line-stops-labels',
                type: 'symbol',
                source: 'selected-line-stops',
                minzoom: 12,
                layout: {
                    'text-field': ['get', 'name'],
                    'text-font': ['Open Sans Bold', 'Arial Unicode MS Bold'],
                    'text-size': 12,
                    'text-offset': [0, 1.8],
                    'text-anchor': 'top'
                },
                paint: {
                    'text-color': '#ff6b00',
                    'text-halo-color': '#fff',
                    'text-halo-width': 2
                }
            });
        }

        if (!this.map.getLayer('stops-labels')) {
            this.map.addLayer({
                id: 'stops-labels',
                type: 'symbol',
                source: 'stops',
                minzoom: 14,
                layout: {
                    'text-field': ['get', 'name'],
                    'text-font': ['Open Sans Bold', 'Arial Unicode MS Bold'],
                    'text-size': 11,
                    'text-offset': [0, 1.5],
                    'text-anchor': 'top'
                },
                paint: {
                    'text-color': '#333',
                    'text-halo-color': '#fff',
                    'text-halo-width': 2
                }
            });
        }

        if (!this._eventListenersSetup) {
            this.map.on('click', 'vehicles-layer', (e) => this.onVehicleClick(e));
            this.map.on('click', 'stops-layer', (e) => this.onStopClick(e));
            this.map.on('click', 'selected-line-stops-layer', (e) => this.onStopClick(e));

            ['vehicles-layer', 'stops-layer', 'selected-line-stops-layer'].forEach(layer => {
                this.map.on('mouseenter', layer, () => {
                    this.map.getCanvas().style.cursor = 'pointer';
                });
                this.map.on('mouseleave', layer, () => {
                    this.map.getCanvas().style.cursor = '';
                });
            });

            this._eventListenersSetup = true;
        }

        console.log('✅ Map layers setup complete');
    }

    setupLineShapes() {
        console.log('🛤️  Setting up line shapes...');

        if (!this.map.getSource('line-shapes')) {
            this.map.addSource('line-shapes', {
                type: 'geojson',
                data: { type: 'FeatureCollection', features: [] }
            });
        }

        if (!this.map.getLayer('line-shapes-layer')) {
            this.map.addLayer({
                id: 'line-shapes-layer',
                type: 'line',
                source: 'line-shapes',
                layout: {
                    'line-join': 'round',
                    'line-cap': 'round'
                },
                paint: {
                    'line-color': ['get', 'color'],
                    'line-width': [
                        'interpolate', ['linear'], ['zoom'],
                        10, 2, 15, 4, 18, 6
                    ],
                    'line-opacity': 0.7
                }
            }, 'vehicles-layer');
        }

        if (!this._shapeEventListenersSetup) {
            this.map.on('click', 'line-shapes-layer', (e) => {
                const feature = e.features[0];
                const props = feature.properties;

                const operatorBadge = this.getOperatorBadge(props.operator);

                this.closeCurrentPopup();
                this.currentPopup = new mapboxgl.Popup()
                    .setLngLat(e.lngLat)
                    .setHTML(`
                        <div class="popup-title">Line ${props.line_code} ${operatorBadge}</div>
                        <div class="popup-section">
                            <span class="popup-label">Name:</span> ${props.line_name}
                        </div>
                        <div class="popup-section">
                            <span class="popup-label">Route ID:</span> ${props.route_id}
                        </div>
                    `)
                    .addTo(this.map);
            });

            this.map.on('mouseenter', 'line-shapes-layer', () => {
                this.map.getCanvas().style.cursor = 'pointer';
            });
            this.map.on('mouseleave', 'line-shapes-layer', () => {
                this.map.getCanvas().style.cursor = '';
            });

            this._shapeEventListenersSetup = true;
        }

        console.log('✅ Line shapes setup complete');
    }

    async loadNetworkData() {
        try {
            console.log('📡 Fetching network data from:', `${this.apiEndpoint}/network`);

            const response = await fetch(`${this.apiEndpoint}/network`);

            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }

            const json = await response.json();
            console.log('✅ Raw API response received');

            if (json.success && json.data) {
                this.networkData = json.data;
            } else if (json.stops && json.lines) {
                this.networkData = json;
            } else {
                throw new Error(json.error || 'Invalid response structure');
            }

            if (!this.networkData.stops || !this.networkData.lines) {
                throw new Error('Response missing stops or lines data');
            }

            const operators = {};
            this.networkData.lines.forEach(line => {
                operators[line.operator] = (operators[line.operator] || 0) + 1;
            });

            console.log('📊 Network data loaded:');
            console.log(`   • ${this.networkData.stops.length} stops`);
            console.log(`   • ${this.networkData.lines.length} lines`);
            Object.entries(operators).forEach(([op, count]) => {
                console.log(`     - ${op}: ${count} lines`);
            });
            console.log(`   • ${this.networkData.shapes ? Object.keys(this.networkData.shapes).length : 0} shapes`);

            this.cachedStopGraph = null;
            this.cachedLinesByType = null;

            requestAnimationFrame(() => {
                this.updateMap();
                this.updateLineShapes();
                this.updateLegend();
                this.updateStats();
                this.showUpdateIndicator();
            });
        } catch (error) {
            console.error('❌ Error loading network data:', error);
            this.showError(`Failed to load network data: ${error.message}`);
        }
    }

    updateMap() {
        if (!this.networkData) {
            console.error('❌ No network data available');
            return;
        }

        console.log('🗺️  Updating map with network data...');

        const vehicleFeatures = [];
        this.networkData.lines.forEach(line => {
            if (!line.real_time || !Array.isArray(line.real_time)) {
                return;
            }

            line.real_time.forEach(vehicle => {
                if (!vehicle.latitude || !vehicle.longitude ||
                    vehicle.latitude === 0 || vehicle.longitude === 0) {
                    return;
                }

                const hexColor = line.color || '808080';
                const vehicleColor = `#${hexColor}`;

                vehicleFeatures.push({
                    type: 'Feature',
                    geometry: {
                        type: 'Point',
                        coordinates: [vehicle.longitude, vehicle.latitude]
                    },
                    properties: {
                        vehicle_id: vehicle.vehicle_id || 'unknown',
                        trip_id: vehicle.trip_id || 'unknown',
                        route_id: vehicle.route_id || '',
                        destination: vehicle.destination || 'Unknown',
                        line_code: line.line_code,
                        line_name: line.line_name,
                        operator: line.operator,
                        color: hexColor,
                        vehicleColor: vehicleColor,
                        timestamp: vehicle.timestamp || null,
                        delay: vehicle.delay || null
                    }
                });
            });
        });

        console.log(`   ✅ Rendering ${vehicleFeatures.length} vehicles on map`);

        this.map.getSource('vehicles').setData({
            type: 'FeatureCollection',
            features: vehicleFeatures
        });

        const stopFeatures = this.networkData.stops.map(stop => ({
            type: 'Feature',
            geometry: {
                type: 'Point',
                coordinates: [stop.longitude, stop.latitude]
            },
            properties: {
                stop_id: stop.stop_id,
                name: stop.stop_name,
                lines: (stop.lines || []).join(', '),
                alerts: (stop.alerts || []).length,
                real_time_count: (stop.real_time || []).length
            }
        }));

        console.log(`   ✅ Rendering ${stopFeatures.length} stops on map`);

        this.map.getSource('stops').setData({
            type: 'FeatureCollection',
            features: stopFeatures
        });
    }

    updateLineShapes(filterLineRef = null) {
        if (!this.networkData || !this.networkData.shapes || !this.networkData.lines) {
            console.log('⚠️  Cannot update shapes: missing data');
            return;
        }

        const shapeFeatures = [];

        const linesToShow = filterLineRef
            ? this.networkData.lines.filter(l => l.line_ref === filterLineRef)
            : this.networkData.lines;

        linesToShow.forEach(line => {
            if (!line.shape_ids || line.shape_ids.length === 0) return;

            line.shape_ids.forEach(shapeId => {
                const shapePoints = this.networkData.shapes[shapeId];
                if (!shapePoints || shapePoints.length < 2) return;

                const coordinates = shapePoints.map(point => [
                    point.longitude,
                    point.latitude
                ]);

                shapeFeatures.push({
                    type: 'Feature',
                    geometry: {
                        type: 'LineString',
                        coordinates: coordinates
                    },
                    properties: {
                        line_code: line.line_code,
                        line_name: line.line_name,
                        route_id: line.route_id,
                        operator: line.operator,
                        color: `#${line.color || '808080'}`,
                        shape_id: shapeId,
                        line_ref: line.line_ref
                    }
                });
            });
        });

        console.log(`   ✅ Rendering ${shapeFeatures.length} line shapes on map`);

        if (this.map.getSource('line-shapes')) {
            this.map.getSource('line-shapes').setData({
                type: 'FeatureCollection',
                features: shapeFeatures
            });
        }
    }

    updateSelectedLineStops(lineRef) {
        if (!lineRef || !this.networkData) {
            this.map.getSource('selected-line-stops').setData({
                type: 'FeatureCollection',
                features: []
            });
            this.selectedLineStops = [];
            return;
        }

        const line = this.networkData.lines.find(l => l.line_ref === lineRef);
        if (!line) return;

        const lineStops = this.networkData.stops.filter(stop =>
            stop.lines && stop.lines.some(l => l === lineRef)
        );

        this.selectedLineStops = lineStops;

        const stopFeatures = lineStops.map(stop => ({
            type: 'Feature',
            geometry: {
                type: 'Point',
                coordinates: [stop.longitude, stop.latitude]
            },
            properties: {
                stop_id: stop.stop_id,
                name: stop.stop_name,
                lines: (stop.lines || []).join(', '),
                alerts: (stop.alerts || []).length
            }
        }));

        this.map.getSource('selected-line-stops').setData({
            type: 'FeatureCollection',
            features: stopFeatures
        });
    }

    updateLegend() {
        if (!this.networkData || !this.networkData.lines) {
            return;
        }

        if (this.renderDebounceTimer) {
            clearTimeout(this.renderDebounceTimer);
        }

        this.renderDebounceTimer = setTimeout(() => {
            requestAnimationFrame(() => this._doUpdateLegend());
        }, 100);
    }

    _doUpdateLegend() {
        const container = document.getElementById('linesContainer');
        const searchTerm = document.getElementById('lineSearch').value.toLowerCase();

        const groups = this.groupLinesByType();

        let html = '';

        const typeOrder = Object.entries(this.lineTypes)
            .sort((a, b) => a[1].order - b[1].order)
            .map(([type]) => type);

        typeOrder.forEach(type => {
            if (!groups[type] || groups[type].length === 0) return;

            const config = this.lineTypes[type];
            const lines = groups[type].filter(line =>
                line.line_name.toLowerCase().includes(searchTerm) ||
                line.line_code.toLowerCase().includes(searchTerm)
            );

            if (lines.length === 0) return;

            const isCollapsed = this.collapsedGroups.has(type);

            html += `
                <div class="line-group">
                    <div class="line-group-header ${type}" onclick="tbmMap.toggleGroup('${type}')">
                        <div class="line-group-title">
                            <span>${config.icon}</span>
                            <span>${config.name}</span>
                            <span class="line-group-count">${lines.length}</span>
                        </div>
                        <span class="line-group-toggle ${isCollapsed ? 'collapsed' : ''}">▼</span>
                    </div>
                    <div class="line-group-content ${isCollapsed ? 'collapsed' : ''}">
                        <div class="line-group-actions">
                            <button class="group-action-btn" onclick="tbmMap.showGroupShapes('${type}')">
                                Show Routes
                            </button>
                            <button class="group-action-btn" onclick="tbmMap.hideGroupShapes('${type}')">
                                Hide Routes
                            </button>
                        </div>
                        ${lines.map(line => this.renderLineItem(line)).join('')}
                    </div>
                </div>
            `;
        });

        container.innerHTML = html || '<div class="loading">No lines found</div>';
    }

    renderLineItem(line) {
        const vehicleCount = (line.real_time || []).length;
        const alertCount = (line.alerts || []).length;
        const shapeCount = (line.shape_ids || []).length;
        const rgb = this.hexToRgb(line.color || '808080');
        const textColor = this.getContrastColor(rgb);
        const isSelected = this.selectedLine === line.line_ref;

        const operatorBadge = this.getOperatorBadge(line.operator, true);

        return `
            <div class="line-item ${isSelected ? 'selected' : ''}" onclick="tbmMap.selectLine('${line.line_ref}')">
                <div class="line-badge" style="background-color: #${line.color || '808080'}; color: ${textColor}">
                    ${line.line_code}
                </div>
                <div class="line-info">
                    <div class="line-name">${line.line_name} ${operatorBadge}</div>
                    <div class="line-meta">
                        <span class="vehicle-count">🚌 ${vehicleCount}</span>
                        <span class="shape-count">🛤️ ${shapeCount}</span>
                        ${alertCount > 0 ? `<span class="alert-badge">⚠️ ${alertCount}</span>` : ''}
                    </div>
                </div>
            </div>
        `;
    }

    toggleGroup(type) {
        if (this.collapsedGroups.has(type)) {
            this.collapsedGroups.delete(type);
        } else {
            this.collapsedGroups.add(type);
        }
        this.updateLegend();
    }

    showGroupShapes(type) {
        const groups = this.groupLinesByType();
        if (!groups[type]) return;

        this.updateLineShapes();

        const lineRefs = groups[type].map(l => l.line_ref);

        if (this.map.getLayer('line-shapes-layer')) {
            this.map.setFilter('line-shapes-layer',
                ['in', ['get', 'line_ref'], ['literal', lineRefs]]
            );
        }
    }

    hideGroupShapes(type) {
        const groups = this.groupLinesByType();
        if (!groups[type]) return;

        const lineRefs = groups[type].map(l => l.line_ref);

        if (this.map.getLayer('line-shapes-layer')) {
            this.map.setFilter('line-shapes-layer',
                ['!', ['in', ['get', 'line_ref'], ['literal', lineRefs]]]
            );
        }
    }

    selectLine(lineRef) {
        console.log('✅ Selecting line:', lineRef);
        this.selectedLine = lineRef;

        this.updateLegend();
        this.updateLineShapes(lineRef);
        this.updateSelectedLineStops(lineRef);
        this.focusOnLine(lineRef);

        document.getElementById('clearSelectionBtn').style.display = 'block';

        this.map.setLayoutProperty('stops-layer', 'visibility', 'none');
        this.map.setLayoutProperty('stops-labels', 'visibility', 'none');
    }

    clearSelection() {
        console.log('🔄 Clearing selection');
        this.selectedLine = null;

        this.updateLegend();
        this.updateLineShapes();
        this.updateSelectedLineStops(null);

        document.getElementById('infoPanel').style.display = 'none';
        document.getElementById('clearSelectionBtn').style.display = 'none';

        const showStops = document.getElementById('showStops') ? document.getElementById('showStops').checked : true;
        this.map.setLayoutProperty('stops-layer', 'visibility', showStops ? 'visible' : 'none');
        this.map.setLayoutProperty('stops-labels', 'visibility', showStops ? 'visible' : 'none');

        this.map.flyTo({
            center: [-0.5792, 44.8378],
            zoom: 11,
            pitch: 45,
            bearing: 0,
            duration: 2000
        });
    }

    focusOnLine(lineRef) {
        const line = this.networkData.lines.find(l => l.line_ref === lineRef);
        if (!line) return;

        if (line.shape_ids && line.shape_ids.length > 0 && this.networkData.shapes) {
            const allCoords = [];

            line.shape_ids.forEach(shapeId => {
                const shapePoints = this.networkData.shapes[shapeId];
                if (shapePoints) {
                    shapePoints.forEach(point => {
                        allCoords.push([point.longitude, point.latitude]);
                    });
                }
            });

            if (allCoords.length > 0) {
                const bounds = allCoords.reduce((bounds, coord) => {
                    return bounds.extend(coord);
                }, new mapboxgl.LngLatBounds(allCoords[0], allCoords[0]));

                this.map.fitBounds(bounds, {
                    padding: 50,
                    maxZoom: 14,
                    duration: 2000
                });

                this.showLineInfo(line);
                return;
            }
        }

        if (line.real_time && line.real_time.length > 0) {
            const coordinates = line.real_time
                .filter(v => v.latitude && v.longitude)
                .map(v => [v.longitude, v.latitude]);

            if (coordinates.length > 0) {
                const bounds = coordinates.reduce((bounds, coord) => {
                    return bounds.extend(coord);
                }, new mapboxgl.LngLatBounds(coordinates[0], coordinates[0]));

                this.map.fitBounds(bounds, {
                    padding: 50,
                    maxZoom: 14,
                    duration: 2000
                });
            }
        }

        this.showLineInfo(line);
    }

    showLineInfo(line) {
        const panel = document.getElementById('infoPanel');
        const title = document.getElementById('infoPanelTitle');
        const content = document.getElementById('infoPanelContent');

        const lineType = this.lineTypes[this.classifyLine(line)];
        const operatorBadge = this.getOperatorBadge(line.operator).replace('operator-badge', 'network-badge');

        title.innerHTML = `${lineType.icon} Line ${line.line_code} - ${line.line_name} ${operatorBadge}`;

        let html = `
            <div class="popup-section">
                <span class="popup-label">Type:</span> ${lineType.name}
            </div>
            <div class="popup-section">
                <span class="popup-label">Operator:</span> ${line.operator}
            </div>
            <div class="popup-section">
                <span class="popup-label">Route ID:</span> ${line.route_id}
            </div>
            <div class="popup-section">
                <span class="popup-label">Active Vehicles:</span> ${(line.real_time || []).length}
            </div>
            <div class="popup-section">
                <span class="popup-label">Shapes:</span> ${(line.shape_ids || []).length}
            </div>
            <div class="popup-section">
                <span class="popup-label">Stops on Line:</span> ${this.selectedLineStops.length}
            </div>
            <div class="popup-section">
                <span class="popup-label">Destinations:</span>
                ${(line.destinations || []).map(([dir, dest]) => `<br>→ ${dest}`).join('')}
            </div>
        `;

        if (line.alerts && line.alerts.length > 0) {
            html += `
                <div class="popup-section">
                    <span class="popup-label">⚠️ Alerts:</span>
                    ${line.alerts.map(alert => `
                        <div class="arrival-item" style="background: #fff3cd; margin-top: 5px;">
                            <strong>${alert.text}</strong><br>
                            <small>${alert.description}</small>
                        </div>
                    `).join('')}
                </div>
            `;
        }

        if (this.selectedLineStops.length > 0) {
            html += `
                <div class="popup-section">
                    <span class="popup-label">📍 Stops (${this.selectedLineStops.length}):</span>
                    <div class="stops-list">
                        ${this.selectedLineStops.map(stop => `
                            <div class="stop-item" onclick="tbmMap.focusOnStop('${stop.stop_id}')">
                                <div>
                                    <div class="stop-item-name">${stop.stop_name}</div>
                                    <div class="stop-item-id">ID: ${stop.stop_id}</div>
                                </div>
                                <div class="stop-item-directions" onclick="event.stopPropagation(); tbmMap.setStopAsDirectionPoint('${stop.stop_id}')">
                                    🧭
                                </div>
                            </div>
                        `).join('')}
                    </div>
                </div>
            `;
        }

        content.innerHTML = html;
        panel.style.display = 'block';
    }

    focusOnStop(stopId) {
        const stop = this.networkData.stops.find(s => s.stop_id === stopId);
        if (!stop) return;

        this.map.flyTo({
            center: [stop.longitude, stop.latitude],
            zoom: 16,
            duration: 1500
        });

        setTimeout(() => {
            this.createStopPopup(stop);
        }, 1500);
    }

    createStopPopup(stop) {
        let arrivalsHTML = '';
        if (stop.real_time && stop.real_time.length > 0) {
            const sortedArrivals = stop.real_time
                .filter(rt => rt.timestamp)
                .sort((a, b) => a.timestamp - b.timestamp)
                .slice(0, 5);

            arrivalsHTML = `
                <div class="popup-section">
                    <span class="popup-label">🕐 Next Arrivals:</span>
                    ${sortedArrivals.map(rt => {
                        const time = new Date(rt.timestamp * 1000);
                        const now = new Date();
                        const minutesUntil = Math.floor((time - now) / 60000);
                        const timeStr = minutesUntil < 0 ? 'Arriving' :
                            minutesUntil === 0 ? 'Now' :
                                `${minutesUntil} min`;

                        const line = this.networkData.lines.find(l => l.route_id === rt.route_id);
                        const lineCode = line ? line.line_code : '?';
                        const lineColor = line ? line.color : '808080';
                        const operatorBadge = line ? this.getOperatorBadge(line.operator, true).replace('operator-badge', 'operator-badge" style="margin-left: 8px;') : '';

                        return `
                            <div class="arrival-item">
                                <div>
                                    <span class="line-badge" style="background-color: #${lineColor}; display: inline-block; padding: 2px 6px; border-radius: 3px; font-size: 11px;">
                                        ${lineCode}
                                    </span>
                                    ${operatorBadge}
                                    → ${rt.destination || 'Unknown'}
                                </div>
                                <div class="arrival-time">${timeStr}</div>
                            </div>
                        `;
                    }).join('')}
                </div>
            `;
        }

        this.closeCurrentPopup();
        this.currentPopup = new mapboxgl.Popup()
            .setLngLat([stop.longitude, stop.latitude])
            .setHTML(`
                <div class="popup-title">📍 ${stop.stop_name}</div>
                <div class="popup-section">
                    <span class="popup-label">Stop ID:</span> ${stop.stop_id}
                </div>
                <div class="popup-section">
                    <span class="popup-label">Lines:</span> ${(stop.lines || []).join(', ')}
                </div>
                ${arrivalsHTML}
                <div class="popup-actions">
                    <button class="popup-btn popup-btn-primary" onclick="tbmMap.setStopAsDirectionPoint('${stop.stop_id}', true)">
                        📍 Set as Start
                    </button>
                    <button class="popup-btn popup-btn-secondary" onclick="tbmMap.setStopAsDirectionPoint('${stop.stop_id}', false)">
                        🎯 Set as End
                    </button>
                </div>
            `)
            .addTo(this.map);
    }

    closeCurrentPopup() {
        if (this.currentPopup) {
            this.currentPopup.remove();
            this.currentPopup = null;
        }
    }

    onVehicleClick(e) {
        const feature = e.features[0];
        const props = feature.properties;

        const timestamp = props.timestamp ?
            new Date(props.timestamp * 1000).toLocaleTimeString() : 'Unknown';

        const delayText = props.delay ?
            `<div class="arrival-delay ${props.delay > 0 ? 'late' : ''}">${props.delay > 0 ? '+' : ''}${props.delay}s</div>` : '';

        const operatorBadge = this.getOperatorBadge(props.operator).replace('operator-badge', 'network-badge');

        this.closeCurrentPopup();
        this.currentPopup = new mapboxgl.Popup()
            .setLngLat(feature.geometry.coordinates)
            .setHTML(`
                <div class="popup-title">🚌 Vehicle ${props.vehicle_id}</div>
                <div class="popup-section">
                    <span class="popup-label">Line:</span> 
                    <span class="line-badge" style="background-color: #${props.color}; display: inline-block; padding: 2px 8px; border-radius: 3px;">
                        ${props.line_code}
                    </span> ${props.line_name} ${operatorBadge}
                </div>
                <div class="popup-section">
                    <span class="popup-label">Destination:</span> ${props.destination}
                </div>
                <div class="popup-section">
                    <span class="popup-label">Last Update:</span> ${timestamp}
                    ${delayText}
                </div>
            `)
            .addTo(this.map);
    }

    onStopClick(e) {
        const feature = e.features[0];
        const stopId = feature.properties.stop_id;
        const stop = this.networkData.stops.find(s => s.stop_id === stopId);

        if (!stop) return;

        this.createStopPopup(stop);
    }

    setupEventListeners() {
        const desktopCheckboxes = ['showShapes', 'showVehicles', 'showStops', 'showAlerts', 'showHeatmap'];
        
        desktopCheckboxes.forEach(id => {
            const checkbox = document.getElementById(id);
            if (!checkbox) return;

            if (id === 'showShapes') {
                checkbox.addEventListener('change', (e) => {
                    this.map.setLayoutProperty('line-shapes-layer', 'visibility',
                        e.target.checked ? 'visible' : 'none');
                });
            } else if (id === 'showVehicles') {
                checkbox.addEventListener('change', (e) => {
                    const visibility = e.target.checked ? 'visible' : 'none';
                    this.map.setLayoutProperty('vehicles-layer', 'visibility', visibility);
                    this.map.setLayoutProperty('vehicles-labels', 'visibility', visibility);
                });
            } else if (id === 'showStops') {
                checkbox.addEventListener('change', (e) => {
                    const visibility = e.target.checked ? 'visible' : 'none';
                    if (!this.selectedLine) {
                        this.map.setLayoutProperty('stops-layer', 'visibility', visibility);
                        this.map.setLayoutProperty('stops-labels', 'visibility', visibility);
                    }
                });
            } else if (id === 'showAlerts') {
                checkbox.addEventListener('change', (e) => {
                    if (e.target.checked) {
                        this.map.setFilter('stops-layer', null);
                    } else {
                        this.map.setFilter('stops-layer', ['==', ['get', 'alerts'], 0]);
                    }
                });
            } else if (id === 'showHeatmap') {
                checkbox.addEventListener('change', (e) => {
                    this.map.setLayoutProperty('vehicles-heatmap', 'visibility',
                        e.target.checked ? 'visible' : 'none');
                    this.map.setLayoutProperty('vehicles-layer', 'visibility',
                        e.target.checked ? 'none' : 'visible');
                    this.map.setLayoutProperty('vehicles-labels', 'visibility',
                        e.target.checked ? 'none' : 'visible');
                });
            }
        });

        const lineSearch = document.getElementById('lineSearch');
        if (lineSearch) {
            lineSearch.addEventListener('input', () => {
                this.updateLegend();
            });
        }

        const searchBox = document.getElementById('searchBox');
        if (searchBox) {
            searchBox.addEventListener('input', (e) => {
                if (this.searchDebounceTimer) {
                    clearTimeout(this.searchDebounceTimer);
                }

                this.searchDebounceTimer = setTimeout(() => {
                    this.handleSearch(e.target.value);
                }, 300);
            });
        }
    }

    handleSearch(query) {
        if (!query || query.length < 2) return;

        const lowerQuery = query.toLowerCase();

        const matchingStops = this.networkData.stops.filter(stop =>
            stop.stop_name.toLowerCase().includes(lowerQuery) ||
            stop.stop_id.toLowerCase().includes(lowerQuery)
        );

        const matchingLines = this.networkData.lines.filter(line =>
            line.line_name.toLowerCase().includes(lowerQuery) ||
            line.line_code.toLowerCase().includes(lowerQuery)
        );

        if (matchingStops.length > 0) {
            this.focusOnStop(matchingStops[0].stop_id);
        } else if (matchingLines.length > 0) {
            this.selectLine(matchingLines[0].line_ref);
        }
    }

    updateStats() {
        if (!this.networkData || !this.networkData.lines) return;

        const totalVehicles = this.networkData.lines.reduce((sum, line) =>
            sum + (line.real_time || []).length, 0
        );

        const totalAlerts = this.networkData.lines.reduce((sum, line) =>
            sum + (line.alerts || []).length, 0
        ) + this.networkData.stops.reduce((sum, stop) =>
            sum + (stop.alerts || []).length, 0
        );

        const vehicleCountEl = document.getElementById('vehicleCount');
        const alertCountEl = document.getElementById('alertCount');

        if (vehicleCountEl) vehicleCountEl.textContent = totalVehicles;
        if (alertCountEl) alertCountEl.textContent = totalAlerts;
    }

    startAutoRefresh() {
        this.updateInterval = setInterval(() => {
            console.log('🔄 Auto-refreshing data...');
            this.loadNetworkData();
        }, 30000);
    }

    showUpdateIndicator() {
        const indicator = document.getElementById('updateIndicator');
        indicator.classList.add('show');
        setTimeout(() => {
            indicator.classList.remove('show');
        }, 2000);
    }

    showError(message) {
        const container = document.getElementById('linesContainer');
        container.innerHTML = `
            <div class="error-container">
                <strong>⚠️ Connection Error</strong>
                <p>${message}</p>
                <button onclick="tbmMap.loadNetworkData()">🔄 Retry Connection</button>
            </div>
        `;
    }

    hexToRgb(hex) {
        hex = hex.replace('#', '');
        const result = /^([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
        return result ? {
            r: parseInt(result[1], 16),
            g: parseInt(result[2], 16),
            b: parseInt(result[3], 16)
        } : { r: 128, g: 128, b: 128 };
    }

    getContrastColor(rgb) {
        const luminance = (0.299 * rgb.r + 0.587 * rgb.g + 0.114 * rgb.b) / 255;
        return luminance > 0.5 ? '#000000' : '#ffffff';
    }
}

// Global functions for HTML onclick handlers
function refreshData() {
    tbmMap.loadNetworkData();
}

function centerMap() {
    tbmMap.clearSelection();
}

function toggleDirections() {
    const panel = document.getElementById('directionsPanel');
    if (tbmMap.directionsVisible) {
        tbmMap.clearTransitRoute();
        panel.style.display = 'none';
        tbmMap.directionsVisible = false;
    } else {
        tbmMap.showNotification('🧭 Click two stops to plan your route');
        panel.style.display = 'block';
        tbmMap.directionsVisible = true;
    }
}

function closeInfoPanel() {
    document.getElementById('infoPanel').style.display = 'none';
}

// Initialize
let tbmMap;
window.addEventListener('DOMContentLoaded', () => {
    console.log('🚀 NVT Transit Map Initializing...');
    console.log('📅 Build: 2025-11-20 06:41:53 UTC');
    console.log('👤 User: Cyclolysisss');
    console.log('🌐 Version: 1.2.0 - Dark Mode Fix + Mobile Dropdown + Performance Optimization');
    tbmMap = new TBMTransitMap();
});
