// Jaculus project for ESP32-S3
// Advanced RGB LED and WiFi example

const GPIO = require('gpio');
const WiFi = require('wifi');

// ESP32-S3 specific RGB LED pins (example board)
const RGB_LED = {
    RED: 47,
    GREEN: 21,
    BLUE: 48
};

// Setup RGB LED pins
GPIO.setup(RGB_LED.RED, GPIO.OUT);
GPIO.setup(RGB_LED.GREEN, GPIO.OUT);
GPIO.setup(RGB_LED.BLUE, GPIO.OUT);

let colorIndex = 0;
const colors = [
    { r: 1, g: 0, b: 0 }, // Red
    { r: 0, g: 1, b: 0 }, // Green
    { r: 0, g: 0, b: 1 }, // Blue
    { r: 1, g: 1, b: 0 }, // Yellow
    { r: 1, g: 0, b: 1 }, // Magenta
    { r: 0, g: 1, b: 1 }, // Cyan
    { r: 1, g: 1, b: 1 }, // White
    { r: 0, g: 0, b: 0 }  // Off
];

function setRgbColor(color) {
    GPIO.write(RGB_LED.RED, color.r);
    GPIO.write(RGB_LED.GREEN, color.g);
    GPIO.write(RGB_LED.BLUE, color.b);
}

function cycleColors() {
    const color = colors[colorIndex];
    setRgbColor(color);
    
    const colorNames = ['Red', 'Green', 'Blue', 'Yellow', 'Magenta', 'Cyan', 'White', 'Off'];
    console.log(`ESP32-S3 RGB LED: ${colorNames[colorIndex]}`);
    
    colorIndex = (colorIndex + 1) % colors.length;
}

// Initialize
console.log('Starting ESP32-S3 Jaculus RGB LED demo...');
console.log('Board: ESP32-S3');
console.log('Features: RGB LED, WiFi capable');

// Cycle through colors every 2 seconds
setInterval(cycleColors, 2000);