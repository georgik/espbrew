// IoT Sensor Data Collection with Jaculus
// This example demonstrates HTTP server and sensor reading

const GPIO = require('@jaculus/gpio');
const WiFi = require('@jaculus/wifi');
const HTTP = require('@jaculus/http');

// Configuration
const WIFI_SSID = 'YourWiFiNetwork';
const WIFI_PASSWORD = 'YourPassword';
const SENSOR_PIN = 34; // Analog pin for sensor
const LED_PIN = 2;     // Status LED

// Initialize components
let server;
let sensorData = {
    temperature: 0,
    humidity: 0,
    lastUpdate: new Date().toISOString()
};

function initGPIO() {
    GPIO.setup(LED_PIN, GPIO.OUT);
    GPIO.setup(SENSOR_PIN, GPIO.IN);
    console.log('GPIO initialized');
}

function readSensors() {
    // Simulate sensor readings (in real project, read from actual sensors)
    const analogValue = GPIO.read(SENSOR_PIN) || Math.random() * 4095;
    
    sensorData.temperature = Math.round((analogValue / 4095 * 100) * 10) / 10;
    sensorData.humidity = Math.round((Math.random() * 40 + 30) * 10) / 10;
    sensorData.lastUpdate = new Date().toISOString();
    
    console.log(`Sensor data: T=${sensorData.temperature}°C, H=${sensorData.humidity}%`);
}

function startWebServer() {
    server = HTTP.createServer((req, res) => {
        res.setHeader('Content-Type', 'application/json');
        res.setHeader('Access-Control-Allow-Origin', '*');
        
        if (req.url === '/api/sensors') {
            res.writeHead(200);
            res.end(JSON.stringify(sensorData, null, 2));
        } else if (req.url === '/') {
            res.writeHead(200, {'Content-Type': 'text/html'});
            res.end(`
                <html>
                <head><title>ESP32 Sensor Data</title></head>
                <body>
                    <h1>ESP32 Jaculus Sensor Data</h1>
                    <p>Temperature: ${sensorData.temperature}°C</p>
                    <p>Humidity: ${sensorData.humidity}%</p>
                    <p>Last Update: ${sensorData.lastUpdate}</p>
                    <script>setTimeout(() => location.reload(), 5000);</script>
                </body>
                </html>
            `);
        } else {
            res.writeHead(404);
            res.end('Not Found');
        }
    });
    
    server.listen(80, () => {
        console.log('Web server started on port 80');
    });
}

function blinkStatusLED() {
    static let ledState = false;
    ledState = !ledState;
    GPIO.write(LED_PIN, ledState ? 1 : 0);
}

// Main application
console.log('Starting Jaculus IoT Sensor Application...');

initGPIO();

// Connect to WiFi
WiFi.connect(WIFI_SSID, WIFI_PASSWORD, (err) => {
    if (err) {
        console.error('WiFi connection failed:', err);
        return;
    }
    
    console.log('Connected to WiFi');
    console.log('IP Address:', WiFi.localIP());
    
    startWebServer();
});

// Read sensors every 10 seconds
setInterval(readSensors, 10000);

// Blink status LED every second
setInterval(blinkStatusLED, 1000);

// Initial sensor reading
readSensors();