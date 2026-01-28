#!/usr/bin/env python3
"""
Memory profiling script for takeashot screenshot tool
"""

import sys
import time
import tracemalloc
import gc
import psutil
import os
from PyQt6.QtWidgets import QApplication
from PyQt6.QtCore import QTimer
from main import ScreenshotApp

def capture_memory_usage(label):
    """Capture current memory usage"""
    process = psutil.Process(os.getpid())
    memory_info = process.memory_info()
    current, peak = tracemalloc.get_traced_memory()
    
    print(f"\n=== {label} ===")
    print(f"RSS Memory: {memory_info.rss / 1024 / 1024:.1f} MB")
    print(f"VMS Memory: {memory_info.vms / 1024 / 1024:.1f} MB") 
    print(f"Traced Current: {current / 1024 / 1024:.1f} MB")
    print(f"Traced Peak: {peak / 1024 / 1024:.1f} MB")
    
    # Show top memory allocations
    snapshot = tracemalloc.take_snapshot()
    top_stats = snapshot.statistics('lineno')
    print("\nTop 10 memory allocations:")
    for stat in top_stats[:10]:
        print(f"{stat}")
    
    return memory_info.rss

def main():
    # Start memory tracing
    tracemalloc.start()
    
    print("Starting memory analysis of takeashot...")
    
    # Create application
    app = QApplication(sys.argv)
    screenshot_app = ScreenshotApp()
    
    # Wait for app to initialize
    QTimer.singleShot(1000, lambda: capture_memory_usage("After App Initialization"))
    
    # Function to perform multiple screenshot cycles
    def perform_screenshot_cycles():
        print("\n" + "="*50)
        print("PERFORMING SCREENSHOT CYCLE TEST")
        print("="*50)
        
        initial_memory = capture_memory_usage("Before Screenshot Cycle")
        
        for i in range(3):  # Do 3 screenshot cycles
            print(f"\n--- Screenshot Cycle {i+1} ---")
            
            # Start capture
            screenshot_app.start_capture()
            
            # Wait for capture to complete, then simulate selection
            QTimer.singleShot(2000 * (i+1), lambda i=i: capture_memory_usage(f"After Capture {i+1}"))
            
            # Simulate closing snippers
            QTimer.singleShot(3000 * (i+1), lambda i=i: (
                screenshot_app.close_all_snippers(),
                capture_memory_usage(f"After Close {i+1}")
            ))
        
        # Final memory check after all cycles
        QTimer.singleShot(5000, lambda: (
            capture_memory_usage("After All Cycles"),
            analyze_memory_growth(initial_memory)
        ))
        
        # Force garbage collection
        QTimer.singleShot(6000, lambda: (
            gc.collect(),
            capture_memory_usage("After GC")
        ))
        
        # Exit after analysis
        QTimer.singleShot(8000, app.quit)
    
    def analyze_memory_growth(initial_memory):
        final_memory = psutil.Process(os.getpid()).memory_info().rss
        growth = final_memory - initial_memory
        print(f"\n=== MEMORY GROWTH ANALYSIS ===")
        print(f"Initial: {initial_memory / 1024 / 1024:.1f} MB")
        print(f"Final: {final_memory / 1024 / 1024:.1f} MB") 
        print(f"Growth: {growth / 1024 / 1024:.1f} MB")
        
        if growth > 50 * 1024 * 1024:  # 50MB threshold
            print("⚠️  SIGNIFICANT MEMORY LEAK DETECTED!")
            
            # Get detailed allocation snapshot
            snapshot = tracemalloc.take_snapshot()
            top_stats = snapshot.statistics('lineno')
            print("\nTop memory allocations that may indicate leaks:")
            for stat in top_stats[:20]:
                if stat.size / 1024 / 1024 > 1:  # Only show > 1MB
                    print(f"{stat}")
        else:
            print("✅ Memory growth within normal limits")
    
    # Start the test after app is ready
    QTimer.singleShot(2000, perform_screenshot_cycles)
    
    # Run the application
    sys.exit(app.exec())

if __name__ == "__main__":
    main()