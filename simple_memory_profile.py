#!/usr/bin/env python3
"""
Simple memory profiling for takeashot without external dependencies
"""

import sys
import time
import tracemalloc
import gc
from PyQt6.QtWidgets import QApplication
from PyQt6.QtCore import QTimer

def capture_memory_usage(label):
    """Capture current memory usage"""
    current, peak = tracemalloc.get_traced_memory()
    
    print(f"\n=== {label} ===")
    print(f"Traced Current: {current / 1024 / 1024:.1f} MB")
    print(f"Traced Peak: {peak / 1024 / 1024:.1f} MB")
    
    # Show top memory allocations
    snapshot = tracemalloc.take_snapshot()
    top_stats = snapshot.statistics('lineno')
    print("\nTop 5 memory allocations:")
    for stat in top_stats[:5]:
        print(f"  {stat.size / 1024 / 1024:.1f} MB: {stat}")
    
    return current

def analyze_specific_allocations():
    """Analyze specific allocations that might be causing memory leaks"""
    snapshot = tracemalloc.take_snapshot()
    top_stats = snapshot.statistics('lineno')
    
    # Look for allocations related to images, pixmaps, screenshots
    relevant_stats = []
    for stat in top_stats:
        if any(keyword in str(stat.traceback) for keyword in 
               ['QPixmap', 'QImage', 'screenshot', 'capture', 'image', 'pixmap']):
            relevant_stats.append(stat)
    
    if relevant_stats:
        print("\n=== IMAGE/PIXMAP RELATED ALLOCATIONS ===")
        for stat in relevant_stats[:10]:
            print(f"{stat.size / 1024 / 1024:.1f} MB: {stat}")
    
    return relevant_stats

def main():
    tracemalloc.start()
    
    print("Starting memory analysis of takeashot...")
    
    app = QApplication(sys.argv)
    
    # Import after app creation
    from main import ScreenshotApp
    screenshot_app = ScreenshotApp()
    
    def perform_memory_test():
        print("\n" + "="*50)
        print("SCREENSHOT MEMORY LEAK TEST")
        print("="*50)
        
        initial_memory = capture_memory_usage("Initial State")
        
        def screenshot_cycle():
            # Start screenshot capture
            print("\n--- Starting Screenshot Capture ---")
            screenshot_app.start_capture()
            
            # Analyze after capture (2 seconds delay for capture completion)
            QTimer.singleShot(2000, check_after_capture)
        
        def check_after_capture():
            after_capture_memory = capture_memory_usage("After Screenshot Capture")
            analyze_specific_allocations()
            
            # Close all snippers
            QTimer.singleShot(1000, close_and_analyze)
        
        def close_and_analyze():
            print("\n--- Closing All Snippers ---")
            screenshot_app.close_all_snippers()
            
            # Analyze after closing
            QTimer.singleShot(1000, analyze_memory_leak)
        
        def analyze_memory_leak():
            after_close_memory = capture_memory_usage("After Closing Snippers")
            analyze_specific_allocations()
            
            # Calculate growth
            growth = after_close_memory - initial_memory
            print(f"\n=== MEMORY GROWTH ===")
            print(f"Initial: {initial_memory / 1024 / 1024:.1f} MB")
            print(f"Final: {after_close_memory / 1024 / 1024:.1f} MB")
            print(f"Growth: {growth / 1024 / 1024:.1f} MB")
            
            if growth > 20 * 1024 * 1024:  # 20MB threshold
                print("🔥 MEMORY LEAK DETECTED!")
                
                # Get detailed comparison
                snapshot = tracemalloc.take_snapshot()
                stats = snapshot.statistics('lineno')
                
                print("\nLARGEST ALLOCATIONS (potential leaks):")
                for stat in stats[:15]:
                    if stat.size / 1024 / 1024 > 1:  # > 1MB
                        print(f"📍 {stat.size / 1024 / 1024:.1f} MB: {stat}")
            
            # Force garbage collection and recheck
            QTimer.singleShot(2000, final_gc_test)
        
        def final_gc_test():
            gc.collect()
            capture_memory_usage("After Garbage Collection")
            
            # Multiple cycles test
            QTimer.singleShot(2000, multiple_cycles_test)
        
        def multiple_cycles_test():
            print("\n" + "="*30)
            print("MULTIPLE CYCLES TEST")
            print("="*30)
            
            # Do 3 rapid screenshot cycles
            cycle_count = 0
            def do_cycle():
                nonlocal cycle_count
                if cycle_count < 3:
                    cycle_count += 1
                    print(f"\n--- Cycle {cycle_count} ---")
                    screenshot_app.start_capture()
                    QTimer.singleShot(1500, close_cycle)
                else:
                    final_analysis()
            
            def close_cycle():
                screenshot_app.close_all_snippers()
                QTimer.singleShot(500, do_cycle)
            
            do_cycle()
        
        def final_analysis():
            capture_memory_usage("After 3 Cycles")
            analyze_specific_allocations()
            
            print("\n" + "="*50)
            print("ANALYSIS COMPLETE")
            print("="*50)
            
            # Exit
            QTimer.singleShot(2000, app.quit)
        
        # Start the test
        QTimer.singleShot(1000, screenshot_cycle)
    
    # Start the memory test
    QTimer.singleShot(3000, perform_memory_test)
    
    sys.exit(app.exec())

if __name__ == "__main__":
    main()