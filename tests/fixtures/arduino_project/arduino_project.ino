// Arduino ESP32 Test Project
// Simple blink example for ESPBrew testing

void setup() {
  // Initialize serial communication at 115200 bits per second:
  Serial.begin(115200);
  
  // Initialize the LED pin as an output:
  pinMode(LED_BUILTIN, OUTPUT);
  
  Serial.println("Arduino ESP32 Test Project started!");
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