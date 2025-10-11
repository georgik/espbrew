// Jaculus project example - JavaScript for ESP32
// This is a simple LED blink example

const GPIO = require('gpio');

// Define LED pin (built-in LED on most ESP32 boards)
const LED_PIN = 2;

// Setup GPIO
GPIO.setup(LED_PIN, GPIO.OUT);

let ledState = false;

function blinkLED() {
    ledState = !ledState;
    GPIO.write(LED_PIN, ledState ? 1 : 0);
    console.log(`LED is now: ${ledState ? 'ON' : 'OFF'}`);
}

// Blink LED every second
console.log('Starting LED blink example on ESP32...');
setInterval(blinkLED, 1000);