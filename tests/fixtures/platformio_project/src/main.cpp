// PlatformIO ESP32 Test Project
// Multi-board compatible blink example for ESPBrew testing

#include <Arduino.h>

// LED pin is defined in platformio.ini for each board
#ifndef LED_BUILTIN
#define LED_BUILTIN 2  // Fallback for ESP32
#endif

void setup() {
    // Initialize serial communication at 115200 bits per second:
    Serial.begin(115200);
    
    // Initialize the LED pin as an output:
    pinMode(LED_BUILTIN, OUTPUT);
    
    // Print board information
    Serial.println("=== PlatformIO ESP32 Test Project ===");
    Serial.printf("Running on: %s\n", ARDUINO_BOARD);
    Serial.printf("LED pin: %d\n", LED_BUILTIN);
    Serial.printf("CPU frequency: %d MHz\n", getCpuFrequencyMhz());
    Serial.printf("Free heap: %d bytes\n", ESP.getFreeHeap());
    
    #ifdef BOARD_ESP32C6
    Serial.println("Board type: ESP32-C6");
    #elif defined(BOARD_ESP32S3)
    Serial.println("Board type: ESP32-S3");
    #elif defined(BOARD_ESP32C3)
    Serial.println("Board type: ESP32-C3");
    #elif defined(BOARD_ESP32S2)
    Serial.println("Board type: ESP32-S2");
    #else
    Serial.println("Board type: Generic ESP32");
    #endif
    
    Serial.println("Project initialized successfully!");
    Serial.println("Starting blink loop...");
}

void loop() {
    // Turn the LED on:
    digitalWrite(LED_BUILTIN, HIGH);
    Serial.println("LED ON");
    
    // Wait for a second:
    delay(1000);
    
    // Turn the LED off:
    digitalWrite(LED_BUILTIN, LOW);
    Serial.println("LED OFF");
    
    // Wait for a second:
    delay(1000);
}