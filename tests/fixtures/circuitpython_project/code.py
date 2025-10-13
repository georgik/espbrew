# CircuitPython ESP32-S3 Test Project
# Simple blink and display example for ESPBrew testing

import board
import digitalio
import time
import microcontroller
import supervisor
import gc

print("=== CircuitPython ESP32-S3 Test Project ===")
print("Running on ESP32-S3")

# Configure built-in LED
led = digitalio.DigitalInOut(board.LED)
led.direction = digitalio.Direction.OUTPUT

# Print system information
print("CPU frequency: {} MHz".format(microcontroller.cpu.frequency // 1000000))
print("Temperature: {:.1f}°C".format(microcontroller.cpu.temperature))
print("Free memory: {} bytes".format(gc.mem_free()))
print("CircuitPython version:", supervisor.runtime.serial_connected)

# Try to get board ID if available
try:
    print("Board ID:", board.board_id)
except AttributeError:
    print("Board ID: Not available")

print("Starting blink loop...")

# Main loop
while True:
    # Turn LED on
    led.value = True
    print("LED ON")
    time.sleep(1)
    
    # Turn LED off
    led.value = False
    print("LED OFF") 
    time.sleep(1)
    
    # Perform garbage collection periodically
    gc.collect()