import sys
import os
from PyQt6.QtWidgets import QApplication
from PyQt6.QtQuickWidgets import QQuickWidget
from PyQt6.QtCore import QUrl, QTimer

def test_qml_loading():
    app = QApplication(sys.argv)
    
    view = QQuickWidget()
    qml_path = os.path.abspath("Toolbar.qml")
    view.setSource(QUrl.fromLocalFile(qml_path))
    
    def check_status():
        if view.status() == QQuickWidget.Status.Error:
            print("QML Errors:")
            for error in view.errors():
                print(error.toString())
            sys.exit(1)
        elif view.status() == QQuickWidget.Status.Ready:
            print("QML Loaded Successfully")
            sys.exit(0)
        else:
            print(f"QML Status: {view.status()}")
            # If still loading, wait? But local file should be fast.
            # If Null, then maybe path is wrong.
            if view.status() == QQuickWidget.Status.Null:
                 print("QML Status is Null")
                 sys.exit(1)

    QTimer.singleShot(100, check_status)
    app.exec()

if __name__ == "__main__":
    test_qml_loading()
