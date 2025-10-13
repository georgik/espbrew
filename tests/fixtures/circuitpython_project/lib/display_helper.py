# CircuitPython Display Helper Library
# Utility functions for managing displays and LEDs

import board
import digitalio
import time
import microcontroller

class LEDController:
    """Simple LED control class"""
    
    def __init__(self, pin=None):
        if pin is None:
            pin = board.LED
        self.led = digitalio.DigitalInOut(pin)
        self.led.direction = digitalio.Direction.OUTPUT
        self.led.value = False
    
    def on(self):
        """Turn LED on"""
        self.led.value = True
    
    def off(self):
        """Turn LED off"""
        self.led.value = False
        
    def toggle(self):
        """Toggle LED state"""
        self.led.value = not self.led.value
    
    def blink(self, count=5, delay=0.5):
        """Blink LED for specified count and delay"""
        for i in range(count):
            self.on()
            time.sleep(delay)
            self.off()
            time.sleep(delay)

def get_system_info():
    """Get ESP32-S3 system information"""
    info = {
        'cpu_frequency': microcontroller.cpu.frequency,
        'temperature': microcontroller.cpu.temperature,
        'platform': 'ESP32-S3 CircuitPython'
    }
    
    # Add board-specific info if available
    try:
        info['board_id'] = board.board_id
    except AttributeError:
        info['board_id'] = 'Unknown'
        
    return info

def format_memory(bytes_value):
    """Format memory value in human readable form"""
    if bytes_value < 1024:
        return f"{bytes_value} bytes"
    elif bytes_value < 1024 * 1024:
        return f"{bytes_value / 1024:.1f} KB" 
    else:
        return f"{bytes_value / (1024 * 1024):.1f} MB"