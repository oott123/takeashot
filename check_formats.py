#!/usr/bin/env python3

from PyQt6.QtGui import QImage

def test_formats():
    print("Available QImage formats:")
    formats = []
    for attr in dir(QImage.Format):
        if 'BGRA' in attr or 'RGB' in attr or 'BGR' in attr:
            formats.append(attr)
    
    for fmt in formats:
        print(f"  {fmt}")

if __name__ == "__main__":
    test_formats()