// Sensor utilities module for ESP32
// Example sensor reading functions

const ADC = require('adc');

class SensorModule {
    constructor() {
        this.adcChannel = 0;
        console.log('Sensor module initialized for ESP32');
    }

    readTemperature() {
        // Simulate temperature sensor reading
        const adcValue = ADC.read(this.adcChannel);
        const voltage = (adcValue / 4095) * 3.3; // Convert to voltage
        const temperature = (voltage - 0.5) * 100; // Convert to Celsius
        return Math.round(temperature * 10) / 10; // Round to 1 decimal
    }

    readHumidity() {
        // Simulate humidity sensor reading
        const baseHumidity = 45;
        const variation = Math.random() * 20 - 10; // ±10%
        return Math.max(0, Math.min(100, baseHumidity + variation));
    }
}

module.exports = SensorModule;