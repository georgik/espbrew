package main

import (
	"machine"
	"time"
)

const (
	// Built-in LED pin on most ESP32 boards
	LED_PIN = machine.GPIO2
)

func main() {
	// Configure the LED pin as output
	LED_PIN.Configure(machine.PinConfig{Mode: machine.PinOutput})
	
	println("Starting TinyGo LED blink on ESP32")
	println("LED connected to GPIO2")
	
	// Blink LED forever
	for {
		println("LED ON")
		LED_PIN.High()
		time.Sleep(500 * time.Millisecond)
		
		println("LED OFF")
		LED_PIN.Low()
		time.Sleep(500 * time.Millisecond)
	}
}