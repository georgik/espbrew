/**
 * ESPBrew SPA - Core Application & Router
 * Hybrid Single-Page Application for ESP32 Board Management
 */

class ESPBrewApp {
    constructor() {
        this.currentComponent = 'dashboard';
        this.components = {
            dashboard: '/static/components/dashboard.html',
            flash: '/static/components/flash.html',
            config: '/static/components/config.html'
        };
        
        // Application state
        this.boardTypes = [];
        this.lastBoardsData = null;
        this.autoRefreshInterval = null;
        this.isRefreshing = false;
        
        // Monitoring state  
        this.currentMonitoringSession = null;
        this.currentWebSocket = null;
        this.logEntryCount = 0;
        this.isAutoScrollEnabled = true;
        
        // Flash state
        this.currentBoardForAssignment = null;
        
        this.init();
    }
    
    async init() {
        console.log('[ESPBrew] Initializing SPA...');
        
        // Set up navigation event listeners
        this.setupNavigation();
        
        // Set up keyboard shortcuts
        this.setupKeyboardShortcuts();
        
        // Load initial component from URL hash or default to dashboard
        const initialComponent = window.location.hash.slice(1) || 'dashboard';
        await this.loadComponent(initialComponent);
        
        // Set up auto-refresh for dashboard component
        this.setupAutoRefresh();
        
        console.log('[ESPBrew] SPA initialized successfully');
    }
    
    setupNavigation() {
        // Handle navigation clicks
        document.addEventListener('click', (e) => {
            const navLink = e.target.closest('.nav-link[data-component]');
            if (navLink) {
                e.preventDefault();
                const component = navLink.getAttribute('data-component');
                this.navigateTo(component);
            }
        });
        
        // Handle browser back/forward
        window.addEventListener('hashchange', () => {
            const component = window.location.hash.slice(1) || 'dashboard';
            this.loadComponent(component, false); // Don't update history again
        });
        
        // Handle modal close events
        document.addEventListener('click', (e) => {
            if (e.target.classList.contains('modal')) {
                this.closeModal(e.target.id);
            }
        });
    }
    
    setupKeyboardShortcuts() {
        document.addEventListener('keydown', (e) => {
            // Monitor modal shortcuts
            const monitorModal = document.getElementById('monitor-modal');
            if (monitorModal && monitorModal.style.display === 'block') {
                if (e.key === 'Escape') {
                    this.closeMonitorModal();
                } else if (e.key === 'c' && (e.ctrlKey || e.metaKey)) {
                    e.preventDefault();
                    this.copyLogs();
                } else if (e.key === 'l' && (e.ctrlKey || e.metaKey)) {
                    e.preventDefault();
                    this.clearLogs();
                } else if (e.key === 'r' && (e.ctrlKey || e.metaKey)) {
                    e.preventDefault();
                    this.resetCurrentBoard();
                }
            }
        });
    }
    
    async navigateTo(component) {
        if (component === this.currentComponent) return;
        
        console.log(`[ESPBrew] Navigating to: ${component}`);
        
        // Update URL hash
        window.location.hash = component;
        
        // Load component
        await this.loadComponent(component, false);
    }
    
    async loadComponent(componentName, updateHistory = true) {
        if (!this.components[componentName]) {
            console.error(`[ESPBrew] Unknown component: ${componentName}`);
            return;
        }
        
        try {
            // Show loading state
            const container = document.getElementById('component-container');
            container.innerHTML = `
                <div class="loading">
                    <div class="loading-spinner"></div>
                    <p>Loading ${componentName} component...</p>
                </div>
            `;
            
            // Update navigation active state
            this.updateNavigationState(componentName);
            
            // Cleanup current component
            this.cleanupComponent(this.currentComponent);
            
            // Load component HTML
            const response = await fetch(this.components[componentName]);
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}: ${response.statusText}`);
            }
            
            const html = await response.text();
            
            // Extract and execute script tags separately (innerHTML doesn't execute scripts)
            const tempDiv = document.createElement('div');
            tempDiv.innerHTML = html;
            
            // Extract script content
            const scripts = tempDiv.querySelectorAll('script');
            const scriptContent = Array.from(scripts).map(script => script.textContent).join('\n');
            
            // Remove script tags from HTML before inserting
            scripts.forEach(script => script.remove());
            
            // Insert HTML content
            container.innerHTML = tempDiv.innerHTML;
            
            // Execute the script content
            if (scriptContent.trim()) {
                console.log(`[ESPBrew] Executing ${componentName} component script`);
                try {
                    // Use Function constructor to execute script in global scope
                    new Function(scriptContent)();
                } catch (error) {
                    console.error(`[ESPBrew] Error executing ${componentName} script:`, error);
                }
            }
            
            // Allow time for the component's JavaScript to be processed
            await new Promise(resolve => setTimeout(resolve, 100));
            
            // Initialize component
            await this.initializeComponent(componentName);
            
            // Update current component
            this.currentComponent = componentName;
            
            console.log(`[ESPBrew] Component loaded: ${componentName}`);
            
        } catch (error) {
            console.error(`[ESPBrew] Failed to load component ${componentName}:`, error);
            
            // Show error state
            document.getElementById('component-container').innerHTML = `
                <div class="error-state">
                    <h2>⚠️ Failed to Load Component</h2>
                    <p>Could not load the ${componentName} component.</p>
                    <p>Error: ${error.message}</p>
                    <button class="btn" onclick="window.espbrew.navigateTo('dashboard')">Return to Dashboard</button>
                </div>
            `;
        }
    }
    
    updateNavigationState(activeComponent) {
        // Remove active class from all nav links
        document.querySelectorAll('.nav-link').forEach(link => {
            link.classList.remove('active');
        });
        
        // Add active class to current component
        const activeLink = document.querySelector(`.nav-link[data-component="${activeComponent}"]`);
        if (activeLink) {
            activeLink.classList.add('active');
        }
    }
    
    async initializeComponent(componentName) {
        console.log(`[ESPBrew] Initializing component: ${componentName}`);
        
        switch (componentName) {
            case 'dashboard':
                await this.initializeDashboard();
                break;
            case 'flash':
                await this.initializeFlash();
                break;
            case 'config':
                await this.initializeConfig();
                break;
        }
    }
    
    cleanupComponent(componentName) {
        console.log(`[ESPBrew] Cleaning up component: ${componentName}`);
        
        // Component-specific cleanup
        switch (componentName) {
            case 'dashboard':
                this.cleanupDashboard();
                break;
            case 'flash':
                this.cleanupFlash();
                break;
            case 'config':
                this.cleanupConfig();
                break;
        }
    }
    
    // Dashboard Component Methods
    async initializeDashboard() {
        console.log('[Dashboard] Initializing...');
        
        // Initialize filters
        if (this.initializeDashboardFilters) {
            this.initializeDashboardFilters();
        }
        
        // Load board types for assignments
        await this.loadBoardTypes();
        
        // Load initial data
        await this.loadData(true);
        
        // Process any pending data that might have been loaded before dashboard methods were available
        if (this.processPendingDashboardData) {
            this.processPendingDashboardData();
        }
        
        // Start auto-refresh
        this.startAutoRefresh();
    }
    
    cleanupDashboard() {
        // Stop auto-refresh when leaving dashboard
        this.stopAutoRefresh();
    }
    
    // Flash Component Methods  
    async initializeFlash() {
        console.log('[Flash] Initializing...');
        // Flash component initialization will be implemented
    }
    
    cleanupFlash() {
        // Flash component cleanup
    }
    
    // Config Component Methods
    async initializeConfig() {
        console.log('[Config] Initializing...');
        // Config component initialization will be implemented
    }
    
    cleanupConfig() {
        // Config component cleanup
    }
    
    // Auto-refresh functionality
    setupAutoRefresh() {
        // Handle visibility changes to pause/resume auto-refresh
        document.addEventListener('visibilitychange', () => {
            if (document.hidden) {
                console.log('[Auto-refresh] Tab hidden, stopping auto-refresh');
                this.stopAutoRefresh();
            } else {
                console.log('[Auto-refresh] Tab visible, resuming auto-refresh');
                if (this.currentComponent === 'dashboard') {
                    this.loadData(true);
                    this.startAutoRefresh();
                }
            }
        });
    }
    
    startAutoRefresh() {
        if (this.currentComponent !== 'dashboard') return;
        
        // Clear any existing interval
        if (this.autoRefreshInterval) {
            clearInterval(this.autoRefreshInterval);
        }
        
        // Set up auto-refresh every 10 seconds
        this.autoRefreshInterval = setInterval(() => {
            if (this.currentComponent === 'dashboard') {
                this.loadData(false);
            }
        }, 10000);
        
        console.log('[Auto-refresh] Started with 10-second interval');
    }
    
    stopAutoRefresh() {
        if (this.autoRefreshInterval) {
            clearInterval(this.autoRefreshInterval);
            this.autoRefreshInterval = null;
            console.log('[Auto-refresh] Stopped');
        }
    }
    
    // Data loading methods (will be expanded)
    async loadBoardTypes() {
        try {
            console.log('Loading board types...');
            const response = await fetch('/api/v1/board-types');
            
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}: ${response.statusText}`);
            }
            
            const data = await response.json();
            this.boardTypes = (data.board_types || []).sort((a, b) => a.name.localeCompare(b.name));
            console.log(`Loaded ${this.boardTypes.length} board types`);
            
        } catch (error) {
            console.error('Failed to load board types:', error);
            this.boardTypes = [];
        }
    }
    
    async loadData(manual = false) {
        if (this.isRefreshing) {
            console.log('Refresh already in progress, skipping');
            return;
        }
        
        this.isRefreshing = true;
        
        try {
            console.log(`[Data] Loading boards data (manual: ${manual})`);
            const response = await fetch('/api/v1/boards');
            
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}: ${response.statusText}`);
            }
            
            const data = await response.json();
            this.lastBoardsData = data.boards || [];
            
            // Update UI if dashboard is active
            if (this.currentComponent === 'dashboard') {
                // Ensure dashboard methods are available
                if (this.updateDashboardUI && typeof this.updateDashboardUI === 'function') {
                    this.updateDashboardUI(this.lastBoardsData);
                } else {
                    console.warn('[Data] Dashboard methods not available yet, storing data for later update');
                    // Store data for when dashboard methods become available
                    this.pendingBoardsData = this.lastBoardsData;
                }
            }
            
            console.log(`[Data] Loaded ${this.lastBoardsData.length} boards`);
            
        } catch (error) {
            console.error('Failed to load board data:', error);
            
            if (this.currentComponent === 'dashboard') {
                // Show error in dashboard
                const container = document.getElementById('boards-container');
                if (container) {
                    container.innerHTML = `
                        <div class="error-state">
                            <h3>⚠️ Failed to Load Boards</h3>
                            <p>Error: ${error.message}</p>
                            <button class="btn" onclick="window.espbrew.loadData(true)">Retry</button>
                        </div>
                    `;
                }
            }
        } finally {
            this.isRefreshing = false;
        }
    }
    
    // updateDashboardUI will be implemented by the dashboard component
    
    // Modal methods (shared across components)
    closeModal(modalId) {
        const modal = document.getElementById(modalId);
        if (modal) {
            modal.style.display = 'none';
        }
    }
    
    closeAssignmentModal() {
        this.closeModal('assignment-modal');
        this.currentBoardForAssignment = null;
    }
    
    closeMonitorModal() {
        this.closeModal('monitor-modal');
        if (this.currentWebSocket) {
            this.currentWebSocket.close();
            this.currentWebSocket = null;
        }
        this.currentMonitoringSession = null;
    }
    
    // Monitor methods - Full WebSocket implementation
    startMonitoring(boardId) {
            console.log(`[Monitor] Starting monitoring for board: ${boardId}`);
            
            const board = (this.lastBoardsData || []).find(b => b.id === boardId);
            if (!board) {
                alert('Board not found');
                return;
            }
            
            // Set current monitoring session
            this.currentMonitoringSession = {
                boardId: boardId,
                board: board,
                sessionId: `session_${Date.now()}`,
                startTime: new Date()
            };
            
            // Update modal content
            document.getElementById('monitor-board-name').textContent = board.name || board.port || 'Unknown Board';
            document.getElementById('monitor-board-info').textContent = 
                `${board.chip_type || 'Unknown'} • ${board.port} • ${board.mac_address || 'No MAC'}`;
            
            // Clear existing logs
            this.clearLogs();
            
            // Show monitor modal
            document.getElementById('monitor-modal').style.display = 'block';
            
            // Start WebSocket connection
            this.connectMonitorWebSocket(boardId);
    }
    
    connectMonitorWebSocket(boardId) {
            console.log(`[Monitor] Connecting WebSocket for board: ${boardId}`);
            
            // Close existing connection
            if (this.currentWebSocket) {
                this.currentWebSocket.close();
                this.currentWebSocket = null;
            }
            
            // Create WebSocket URL
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${protocol}//${window.location.host}/ws/monitor/${this.currentMonitoringSession.sessionId}`;
            
            console.log(`[Monitor] Connecting to: ${wsUrl}`);
            
            // Create WebSocket connection
            this.currentWebSocket = new WebSocket(wsUrl);
            
            this.currentWebSocket.onopen = () => {
                console.log('[Monitor] WebSocket connected');
                this.addLogEntry('system', '📡 WebSocket connected - monitoring started');
                
                // Send initial monitoring request
                this.currentWebSocket.send(JSON.stringify({
                    type: 'start_monitoring',
                    board_id: boardId,
                    session_id: this.currentMonitoringSession.sessionId
                }));
            };
            
            this.currentWebSocket.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data);
                    this.handleMonitorMessage(data);
                } catch (error) {
                    console.error('[Monitor] Failed to parse WebSocket message:', error);
                    this.addLogEntry('error', `❌ Message parse error: ${error.message}`);
                }
            };
            
            this.currentWebSocket.onclose = (event) => {
                console.log('[Monitor] WebSocket closed:', event.code, event.reason);
                if (event.code !== 1000) { // Not a normal close
                    this.addLogEntry('error', `❌ Connection lost: ${event.reason || 'Unknown error'}`);
                }
            };
            
            this.currentWebSocket.onerror = (error) => {
                console.error('[Monitor] WebSocket error:', error);
                this.addLogEntry('error', '❌ WebSocket connection error');
            };
    }
    
    handleMonitorMessage(data) {
            switch (data.type) {
                case 'log':
                    this.addLogEntry(data.level || 'info', data.message);
                    break;
                case 'board_status':
                    this.updateBoardStatus(data.status);
                    break;
                case 'monitoring_started':
                    this.addLogEntry('system', '🎯 Board monitoring session started');
                    break;
                case 'monitoring_stopped':
                    this.addLogEntry('system', '⏹️ Board monitoring session ended');
                    break;
                case 'error':
                    this.addLogEntry('error', `❌ ${data.message}`);
                    break;
                default:
                    console.log('[Monitor] Unknown message type:', data.type, data);
                    this.addLogEntry('info', `📝 ${data.message || JSON.stringify(data)}`);
            }
    }
    
    addLogEntry(level, message) {
            const logContainer = document.getElementById('log-container');
            if (!logContainer) return;
            
            const timestamp = new Date().toLocaleTimeString();
            const entry = document.createElement('div');
            entry.className = `log-entry ${level}`;
            entry.innerHTML = `<span class="log-timestamp">[${timestamp}]</span> ${message}`;
            
            logContainer.appendChild(entry);
            this.logEntryCount++;
            
            // Auto-scroll if enabled
            if (this.isAutoScrollEnabled) {
                logContainer.scrollTop = logContainer.scrollHeight;
            }
            
            // Limit log entries to prevent memory issues
            const maxEntries = 1000;
            if (this.logEntryCount > maxEntries) {
                const firstEntry = logContainer.firstChild;
                if (firstEntry) {
                    logContainer.removeChild(firstEntry);
                    this.logEntryCount--;
                }
            }
    }
    
    updateBoardStatus(status) {
            console.log('[Monitor] Board status update:', status);
            // Update status indicators in the monitor modal if needed
    }
    
    clearLogs() {
            const logContainer = document.getElementById('log-container');
            if (logContainer) {
                logContainer.innerHTML = '<div class="log-entry system">🗑️ Logs cleared</div>';
                this.logEntryCount = 1;
            }
    }
    
    copyLogs() {
            console.log('[Monitor] Copying logs to clipboard...');
            
            const logContainer = document.getElementById('log-container');
            if (!logContainer) return;
            
            // Extract text content from log entries
            const logText = Array.from(logContainer.children)
                .map(entry => entry.textContent)
                .join('\n');
            
            // Copy to clipboard
            navigator.clipboard.writeText(logText).then(() => {
                this.addLogEntry('system', '📋 Logs copied to clipboard');
            }).catch(error => {
                console.error('Failed to copy logs:', error);
                this.addLogEntry('error', '❌ Failed to copy logs to clipboard');
            });
    }
    
    resetCurrentBoard() {
            if (!this.currentMonitoringSession) return;
            
            console.log('[Monitor] Resetting current board...');
            
            const boardId = this.currentMonitoringSession.boardId;
            
            // Send reset command via API
            fetch('/api/v1/reset', {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify({
                    board_id: boardId
                })
            })
            .then(response => {
                if (!response.ok) {
                    throw new Error(`HTTP ${response.status}: ${response.statusText}`);
                }
                return response.json();
            })
            .then(data => {
                console.log('[Monitor] Board reset successful:', data);
                this.addLogEntry('system', '🔄 Board reset command sent');
            })
            .catch(error => {
                console.error('[Monitor] Board reset failed:', error);
                this.addLogEntry('error', `❌ Reset failed: ${error.message}`);
            });
    }
    
    stopCurrentMonitoring() {
            console.log('[Monitor] Stopping current monitoring session...');
            
            if (this.currentWebSocket) {
                // Send stop monitoring command
                this.currentWebSocket.send(JSON.stringify({
                    type: 'stop_monitoring',
                    session_id: this.currentMonitoringSession?.sessionId
                }));
                
                // Close WebSocket connection
                this.currentWebSocket.close(1000, 'User requested stop');
                this.currentWebSocket = null;
            }
            
            this.addLogEntry('system', '⏹️ Monitoring stopped by user');
            this.currentMonitoringSession = null;
            
            // Close modal
            setTimeout(() => {
                this.closeMonitorModal();
            }, 1000);
    }
    
    toggleAutoScroll() {
            this.isAutoScrollEnabled = !this.isAutoScrollEnabled;
            const checkbox = document.getElementById('auto-scroll-checkbox');
            if (checkbox) {
                checkbox.checked = this.isAutoScrollEnabled;
            }
            
            this.addLogEntry('system', 
                this.isAutoScrollEnabled ? 
                '📜 Auto-scroll enabled' : 
                '📜 Auto-scroll disabled'
            );
    }
    
    downloadLogs() {
            console.log('[Monitor] Downloading logs...');
            
            const logContainer = document.getElementById('log-container');
            if (!logContainer) return;
            
            // Extract text content from log entries
            const logText = Array.from(logContainer.children)
                .map(entry => entry.textContent)
                .join('\n');
            
            // Create and download file
            const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
            const filename = `espbrew-monitor-${this.currentMonitoringSession?.boardId || 'unknown'}-${timestamp}.log`;
            
            const blob = new Blob([logText], { type: 'text/plain' });
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = filename;
            a.click();
            URL.revokeObjectURL(url);
            
            this.addLogEntry('system', `💾 Logs downloaded: ${filename}`);
    }
}

// Initialize the application when DOM is ready
document.addEventListener('DOMContentLoaded', function() {
    console.log('[ESPBrew] DOM loaded, creating SPA instance...');
    window.espbrew = new ESPBrewApp();
});

// Export for global access
if (typeof module !== 'undefined' && module.exports) {
    module.exports = ESPBrewApp;
}