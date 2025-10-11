# MicroPython boot.py
# This file is executed on every boot (including wake-boot from deepsleep)

import esp
import esp32
import gc
import machine
import micropython

print("Booting MicroPython on ESP32...")

# Disable ESP32 debug output
esp.osdebug(None)

# Enable garbage collection  
gc.enable()

# Allocate emergency exception buffer
micropython.alloc_emergency_exception_buf(100)

# Print boot information
print("Boot completed successfully")
print("MicroPython version:", micropython.const(32))  
print("Available memory:", gc.mem_free(), "bytes")

# Optional: Connect to WiFi on boot
# import network
# wlan = network.WLAN(network.STA_IF)
# wlan.active(True)
# if not wlan.isconnected():
#     print('Connecting to WiFi...')
#     wlan.connect('YOUR_SSID', 'YOUR_PASSWORD')
#     while not wlan.isconnected():
#         pass
# print('Network config:', wlan.ifconfig())