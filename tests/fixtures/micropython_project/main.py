# MicroPython ESP32 Test Project
# Simple blink example for ESPBrew testing

import machine
import utime
import esp32
from machine import Pin
import network

print("=== MicroPython ESP32 Test Project ===")
print("Running on ESP32")

# Configure LED pin - ESP32 built-in LED
led = Pin(2, Pin.OUT)

# Print system information  
print("CPU frequency: {} MHz".format(machine.freq() // 1000000))
print("Flash size: {} MB".format(esp32.flash_size() // (1024 * 1024)))
print("Free memory: {} bytes".format(machine.mem_info()[0]))

# Initialize WiFi (but don't connect)
wlan = network.WLAN(network.STA_IF)
print("WiFi MAC address: {}".format(wlan.config('mac')))

print("Starting blink loop...")

# Main loop
while True:
    # Turn LED on
    led.on()
    print("LED ON")
    utime.sleep(1)
    
    # Turn LED off  
    led.off()
    print("LED OFF")
    utime.sleep(1)