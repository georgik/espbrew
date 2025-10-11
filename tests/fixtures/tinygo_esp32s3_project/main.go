package main

import (
	"machine"
	"time"
)

const (
	// ESP32-S3 specific RGB LED pins (example for some boards)
	RGB_RED   = machine.GPIO47
	RGB_GREEN = machine.GPIO21  
	RGB_BLUE  = machine.GPIO48
	
	// ADC pin for sensor reading
	SENSOR_PIN = machine.ADC{Pin: machine.GPIO1}
)

func main() {
	// Configure RGB LED pins
	RGB_RED.Configure(machine.PinConfig{Mode: machine.PinOutput})
	RGB_GREEN.Configure(machine.PinConfig{Mode: machine.PinOutput})
	RGB_BLUE.Configure(machine.PinConfig{Mode: machine.PinOutput})
	
	// Configure ADC for sensor reading
	machine.InitADC()
	SENSOR_PIN.Configure(machine.ADCConfig{})
	
	println("Starting TinyGo sensor demo on ESP32-S3")
	println("RGB LED + Sensor reading example")
	
	colorIndex := 0
	colors := []string{"Red", "Green", "Blue", "Yellow", "Magenta", "Cyan"}
	
	for {
		// Read sensor value
		sensorValue := SENSOR_PIN.Get()
		println("Sensor reading:", sensorValue)
		
		// Cycle through colors based on sensor value
		switch colorIndex % 6 {
		case 0: // Red
			RGB_RED.High()
			RGB_GREEN.Low()
			RGB_BLUE.Low()
		case 1: // Green
			RGB_RED.Low()
			RGB_GREEN.High()
			RGB_BLUE.Low()
		case 2: // Blue
			RGB_RED.Low()
			RGB_GREEN.Low()
			RGB_BLUE.High()
		case 3: // Yellow
			RGB_RED.High()
			RGB_GREEN.High()
			RGB_BLUE.Low()
		case 4: // Magenta
			RGB_RED.High()
			RGB_GREEN.Low()
			RGB_BLUE.High()
		case 5: // Cyan
			RGB_RED.Low()
			RGB_GREEN.High()
			RGB_BLUE.High()
		}
		
		println("ESP32-S3 RGB Color:", colors[colorIndex%6])
		
		colorIndex++
		time.Sleep(1 * time.Second)
	}
}