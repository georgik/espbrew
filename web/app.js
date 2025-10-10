// ESPBrew Dashboard JavaScript

class ESPBrewDashboard {
    constructor() {
        this.boards = [];
        this.currentMonitoringSession = null;
        this.websocket = null;
        this.isMonitoring = false;
        
        this.init();
    }

    async init() {
        console.log('🍺 ESPBrew Dashboard initializing...');
        
        // Check server status
        await this.checkServerStatus();
        
        // Load boards
        await this.loadBoards();
        
        // Setup periodic updates
        setInterval(() => this.checkServerStatus(), 30000);
        setInterval(() => this.loadBoards(), 10000);
    }

    async checkServerStatus() {
        try {
            const response = await fetch('/api/v1/boards');
            this.updateServerStatus(response.ok, response.ok ? 'Server Online' : 'Server Error');
        } catch (error) {
            console.error('Server status check failed:', error);
            this.updateServerStatus(false, 'Server Offline');
        }
    }

    updateServerStatus(isOnline, statusText) {
        const statusDot = document.getElementById('statusDot');
        const statusTextEl = document.getElementById('statusText');
        
        if (statusDot && statusTextEl) {
            statusDot.className = 'status-dot' + (isOnline ? '' : ' error');
            statusTextEl.textContent = statusText;
        }
    }

    async loadBoards() {
        try {
            const response = await fetch('/api/v1/boards');
            if (!response.ok) throw new Error('Failed to load boards');
            
            const data = await response.json();
            this.boards = data.boards || [];
            this.renderBoards();
            this.updateBoardSelect();
        } catch (error) {
            console.error('Failed to load boards:', error);
            this.renderBoardsError();
        }
    }

    renderBoards() {
        const boardsList = document.getElementById('boardsList');
        if (!boardsList) return;

        if (this.boards.length === 0) {
            boardsList.innerHTML = '<div class="loading">No boards available</div>';
            return;
        }

        boardsList.innerHTML = this.boards.map(board => `
            <div class="board-item">
                <div class="board-name">${this.escapeHtml(board.logical_name || board.id)}</div>
                <div class="board-status ${this.getStatusClass(board.status)}">${board.status}</div>
                <div class="board-details">
                    <small>Type: ${this.escapeHtml(board.chip_type)}</small><br>
                    <small>Port: ${this.escapeHtml(board.port)}</small><br>
                    <small>MAC: ${this.escapeHtml(board.id)}</small>
                </div>
            </div>
        `).join('');
    }

    renderBoardsError() {
        const boardsList = document.getElementById('boardsList');
        if (boardsList) {
            boardsList.innerHTML = '<div class="loading">Failed to load boards</div>';
        }
    }

    updateBoardSelect() {
        const select = document.getElementById('boardSelect');
        if (!select) return;

        select.innerHTML = '<option value="">Select a board to monitor...</option>' +
            this.boards.map(board => 
                `<option value="${board.id}">${this.escapeHtml(board.logical_name || board.id)} (${board.status})</option>`
            ).join('');
    }

    getStatusClass(status) {
        switch (status.toLowerCase()) {
            case 'available': return 'available';
            case 'monitoring': return 'monitoring';
            case 'flashing': return 'flashing';
            default: return 'available';
        }
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    // Monitoring Functions
    async startMonitoring() {
        const boardSelect = document.getElementById('boardSelect');
        const selectedBoardId = boardSelect?.value;
        
        if (!selectedBoardId) {
            alert('Please select a board to monitor');
            return;
        }

        const selectedBoard = this.boards.find(b => b.id === selectedBoardId);
        if (!selectedBoard) {
            alert('Selected board not found');
            return;
        }

        try {
            // Start monitoring session
            const response = await fetch('/api/v1/monitor/start', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    board_id: selectedBoardId,
                    baud_rate: 115200,
                    filters: null
                })
            });

            if (!response.ok) throw new Error('Failed to start monitoring');

            const data = await response.json();
            if (!data.success) throw new Error(data.message);

            this.currentMonitoringSession = data.session_id;
            this.connectWebSocket(data.websocket_url);
            
            // Update UI
            this.updateMonitoringUI(true);
            this.addLogLine(`🔗 Connected to ${selectedBoard.logical_name || selectedBoard.id}`, 'info');
            
        } catch (error) {
            console.error('Failed to start monitoring:', error);
            alert('Failed to start monitoring: ' + error.message);
        }
    }

    async stopMonitoring() {
        if (!this.currentMonitoringSession) return;

        try {
            // Close WebSocket
            if (this.websocket) {
                this.websocket.close();
                this.websocket = null;
            }

            // Stop monitoring session
            await fetch('/api/v1/monitor/stop', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    session_id: this.currentMonitoringSession
                })
            });

            this.currentMonitoringSession = null;
            this.updateMonitoringUI(false);
            this.addLogLine('🔌 Monitoring session stopped', 'info');
            
        } catch (error) {
            console.error('Failed to stop monitoring:', error);
        }
    }

    connectWebSocket(websocketUrl) {
        const wsUrl = `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}${websocketUrl}`;
        
        console.log('Connecting to WebSocket:', wsUrl);
        
        this.websocket = new WebSocket(wsUrl);
        
        this.websocket.onopen = () => {
            console.log('WebSocket connected');
            this.addLogLine('📡 WebSocket connected', 'info');
            
            // Send authentication message
            this.websocket.send(JSON.stringify({
                type: 'auth',
                session_id: this.currentMonitoringSession
            }));
        };
        
        this.websocket.onmessage = (event) => {
            try {
                const message = JSON.parse(event.data);
                this.handleWebSocketMessage(message);
            } catch (error) {
                // Handle raw log messages
                this.addLogLine(event.data);
            }
        };
        
        this.websocket.onclose = () => {
            console.log('WebSocket disconnected');
            this.addLogLine('🔌 WebSocket disconnected', 'warning');
            this.websocket = null;
        };
        
        this.websocket.onerror = (error) => {
            console.error('WebSocket error:', error);
            this.addLogLine('❌ WebSocket error occurred', 'error');
        };
    }

    handleWebSocketMessage(message) {
        switch (message.type) {
            case 'connected':
                this.addLogLine(`✅ ${message.message}`, 'info');
                break;
            case 'error':
                this.addLogLine(`❌ ${message.message}`, 'error');
                break;
            case 'pong':
                // Handle pong silently
                break;
            case 'keepalive_ack':
                // Handle keepalive ack silently
                break;
            default:
                // Handle log message
                if (message.content) {
                    const timestamp = new Date(message.timestamp).toLocaleTimeString();
                    this.addLogLine(`[${timestamp}] ${message.content}`, this.detectLogLevel(message.content));
                }
        }
    }

    detectLogLevel(content) {
        const upper = content.toUpperCase();
        if (upper.includes('ERROR') || upper.includes('E (')) return 'error';
        if (upper.includes('WARN') || upper.includes('W (')) return 'warning';
        if (upper.includes('INFO') || upper.includes('I (')) return 'info';
        return '';
    }

    addLogLine(content, level = '') {
        const output = document.getElementById('monitorOutput');
        if (!output) return;

        // Remove "no monitor" message if present
        const noMonitor = output.querySelector('.no-monitor');
        if (noMonitor) noMonitor.remove();

        // Create log line
        const logLine = document.createElement('div');
        logLine.className = `log-line ${level}`;
        logLine.textContent = content;
        
        output.appendChild(logLine);
        
        // Auto-scroll to bottom
        output.scrollTop = output.scrollHeight;
        
        // Limit log lines to prevent memory issues
        const lines = output.querySelectorAll('.log-line');
        if (lines.length > 1000) {
            lines[0].remove();
        }
    }

    updateMonitoringUI(isMonitoring) {
        const startBtn = document.getElementById('startMonitoring');
        const stopBtn = document.getElementById('stopMonitoring');
        const boardSelect = document.getElementById('boardSelect');
        
        this.isMonitoring = isMonitoring;
        
        if (startBtn) {
            startBtn.style.display = isMonitoring ? 'none' : 'block';
            startBtn.disabled = isMonitoring;
        }
        
        if (stopBtn) {
            stopBtn.style.display = isMonitoring ? 'block' : 'none';
            stopBtn.disabled = !isMonitoring;
        }
        
        if (boardSelect) {
            boardSelect.disabled = isMonitoring;
        }
    }
}

// Tab switching functionality
function showTab(tabName) {
    // Hide all tab contents
    document.querySelectorAll('.tab-content').forEach(tab => {
        tab.classList.remove('active');
    });
    
    // Remove active class from all buttons
    document.querySelectorAll('.tab-button').forEach(btn => {
        btn.classList.remove('active');
    });
    
    // Show selected tab
    const selectedTab = document.getElementById(tabName);
    if (selectedTab) {
        selectedTab.classList.add('active');
    }
    
    // Activate corresponding button
    const activeButton = document.querySelector(`[onclick="showTab('${tabName}')"]`);
    if (activeButton) {
        activeButton.classList.add('active');
    }
}

// Global functions for monitoring
let dashboard;

function startMonitoring() {
    if (dashboard) {
        dashboard.startMonitoring();
    }
}

function stopMonitoring() {
    if (dashboard) {
        dashboard.stopMonitoring();
    }
}

// Initialize dashboard when page loads
document.addEventListener('DOMContentLoaded', () => {
    dashboard = new ESPBrewDashboard();
});

// Handle page unload
window.addEventListener('beforeunload', () => {
    if (dashboard && dashboard.isMonitoring) {
        dashboard.stopMonitoring();
    }
});