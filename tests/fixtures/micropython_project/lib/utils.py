# MicroPython utility functions
# Supporting library for the test project

import machine
import utime
from machine import Pin

def blink_led(pin_num=2, count=5, delay=0.5):
    """Blink LED on specified pin for a given count and delay"""
    led = Pin(pin_num, Pin.OUT)
    
    for i in range(count):
        led.on()
        print(f"Blink {i+1}/{count} - LED ON")
        utime.sleep(delay)
        
        led.off()
        print(f"Blink {i+1}/{count} - LED OFF")
        utime.sleep(delay)

def get_system_info():
    """Get ESP32 system information"""
    info = {
        'cpu_freq': machine.freq(),
        'free_memory': machine.mem_info()[0],
        'platform': 'ESP32'
    }
    return info

def deep_sleep(seconds):
    """Put ESP32 into deep sleep for specified seconds"""
    print(f"Going to deep sleep for {seconds} seconds...")
    machine.deepsleep(seconds * 1000)  # Convert to milliseconds