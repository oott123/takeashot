#!/usr/bin/env python3
"""
Detailed memory leak analysis focusing on pixmap/image allocations
"""

import sys
import time
import tracemalloc
import gc
from PyQt6.QtWidgets import QApplication
from PyQt6.QtCore import QTimer
from PyQt6.QtGui import QPixmap, QImage

def analyze_pixmap_leaks():
    """Focus specifically on QPixmap/QImage memory allocations"""
    snapshot = tracemalloc.take_snapshot()
    
    # Filter for relevant allocations
    relevant_stats = []
    for stat in snapshot.statistics('lineno'):
        trace_str = str(stat.traceback)
        if any(keyword in trace_str.lower() for keyword in 
               ['qpixmap', 'qimage', 'image', 'pixmap', 'capture', 'screenshot']):
            relevant_stats.append(stat)
    
    relevant_stats.sort(key=lambda x: x.size, reverse=True)
    
    print("\n=== PIXMAP/IMAGE ALLOCATION ANALYSIS ===")
    print(f"Found {len(relevant_stats)} relevant allocations")
    
    total_image_memory = sum(stat.size for stat in relevant_stats)
    print(f"Total image/pixmap memory: {total_image_memory / 1024 / 1024:.1f} MB")
    
    for i, stat in enumerate(relevant_stats[:10]):
        print(f"  {i+1}. {stat.size / 1024 / 1024:.2f} MB: {stat}")
    
    return relevant_stats

def check_object_counts():
    """Check for object count growth"""
    print(f"\n=== OBJECT COUNT ANALYSIS ===")
    
    # Count PyQt objects that might be leaking
    import gc
    
    # Get all objects and count relevant types
    all_objects = gc.get_objects()
    pixmap_count = sum(1 for obj in all_objects if isinstance(obj, QPixmap))
    image_count = sum(1 for obj in all_objects if isinstance(obj, QImage))
    
    print(f"QPixmap objects: {pixmap_count}")
    print(f"QImage objects: {image_count}")
    
    # Count by traceback to identify where objects are created
    pixmap_traces = {}
    image_traces = {}
    
    for obj in all_objects:
        try:
            if isinstance(obj, QPixmap):
                # Try to get creation info
                pixmap_traces['general'] = pixmap_traces.get('general', 0) + 1
            elif isinstance(obj, QImage):
                image_traces['general'] = image_traces.get('general', 0) + 1
        except:
            pass
    
    return pixmap_count, image_count

def main():
    tracemalloc.start()
    
    print("Starting detailed pixmap memory analysis...")
    
    app = QApplication(sys.argv)
    from main import ScreenshotApp
    screenshot_app = ScreenshotApp()
    
    def detailed_analysis():
        print("\n" + "="*60)
        print("DETAILED PIXMAP MEMORY LEAK ANALYSIS")
        print("="*60)
        
        # Baseline check
        baseline_pixmaps, baseline_images = check_object_counts()
        analyze_pixmap_leaks()
        
        def capture_and_analyze():
            print("\n--- CAPTURING SCREENSHOT ---")
            screenshot_app.start_capture()
            
            QTimer.singleShot(3000, after_capture)
        
        def after_capture():
            print("\n--- AFTER CAPTURE ---")
            after_pixmaps, after_images = check_object_counts()
            allocations = analyze_pixmap_leaks()
            
            print(f"\nPixmaps: {baseline_pixmaps} -> {after_pixmaps} (+{after_pixmaps - baseline_pixmaps})")
            print(f"Images: {baseline_images} -> {after_images} (+{after_images - baseline_images})")
            
            # Close and analyze
            QTimer.singleShot(1000, close_and_analyze)
        
        def close_and_analyze():
            print("\n--- AFTER CLOSING ---")
            screenshot_app.close_all_snippers()
            
            QTimer.singleShot(2000, final_analysis)
        
        def final_analysis():
            final_pixmaps, final_images = check_object_counts()
            allocations = analyze_pixmap_leaks()
            
            print(f"\nPixmaps: {baseline_pixmaps} -> {final_pixmaps} (+{final_pixmaps - baseline_pixmaps})")
            print(f"Images: {baseline_images} -> {final_images} (+{final_images - baseline_images})")
            
            # Look for specific leak patterns
            if final_pixmaps > baseline_pixmaps + 2:  # Allow some tolerance
                print("🔥 POTENTIAL PIXMAP LEAK DETECTED!")
            
            if final_images > baseline_images + 2:
                print("🔥 POTENTIAL IMAGE LEAK DETECTED!")
            
            # Check specific screenshot backend lines
            print("\n=== SCREENSHOT BACKEND ALLOCATIONS ===")
            snapshot = tracemalloc.take_snapshot()
            stats = snapshot.statistics('lineno')
            
            for stat in stats:
                if 'screenshot_backend.py' in str(stat.traceback):
                    print(f"  {stat.size / 1024:.1f} KB: {stat}")
            
            QTimer.singleShot(2000, app.quit)
        
        QTimer.singleShot(2000, capture_and_analyze)
    
    QTimer.singleShot(3000, detailed_analysis)
    sys.exit(app.exec())

if __name__ == "__main__":
    main()