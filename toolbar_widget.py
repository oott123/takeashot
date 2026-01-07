from PyQt5.QtWidgets import QWidget
from PyQt5.QtCore import Qt, QRect
from PyQt5.QtGui import QPainter, QColor, QPen


class ToolbarWidget(QWidget):
    """截图工具条组件"""

    def __init__(self):
        super().__init__()
        self.setWindowFlags(
            Qt.FramelessWindowHint
            | Qt.WindowStaysOnTopHint
            | Qt.Tool
            | Qt.X11BypassWindowManagerHint
        )
        self.setAttribute(Qt.WA_TranslucentBackground)

        # 工具条尺寸
        self.toolbar_width = 200
        self.toolbar_height = 40
        self.resize(self.toolbar_width, self.toolbar_height)

        # 隐藏初始状态
        self.hide()

    def paintEvent(self, event):
        """绘制工具条UI"""
        painter = QPainter(self)
        painter.setRenderHint(QPainter.Antialiasing)

        # 绘制半透明背景
        bg_color = QColor(40, 40, 40, 200)
        painter.fillRect(self.rect(), bg_color)

        # 绘制边框
        pen = QPen(QColor(255, 255, 255, 150), 1)
        painter.setPen(pen)
        painter.drawRect(self.rect().adjusted(0, 0, -1, -1))

        painter.end()
