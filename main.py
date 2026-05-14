import sys
import os
import asyncio
import ctypes
from PyQt6.QtWidgets import QApplication, QMainWindow, QWidget, QLabel, QVBoxLayout, QHBoxLayout, QSystemTrayIcon, QMenu, QGraphicsDropShadowEffect
from PyQt6.QtCore import Qt, QTimer, QPoint, QPointF, QRectF, QSize, pyqtSignal, QObject
from PyQt6.QtGui import QPixmap, QIcon, QAction, QPainter, QTransform, QPainterPath, QColor, QFont, QFontMetrics, QBrush, QPolygonF, QPen

from winrt.windows.media.control import GlobalSystemMediaTransportControlsSessionManager as SessionManager
from winrt.windows.storage.streams import DataReader

def resource_path(relative_path):
    try:
        base_path = sys._MEIPASS
    except Exception:
        base_path = os.path.abspath(".")
    return os.path.join(base_path, relative_path)

import threading
import time

VK_VOLUME_UP = 0xAF
VK_VOLUME_DOWN = 0xAE
KEYEVENTF_KEYUP = 0x0002

_user32 = ctypes.windll.user32

def _volume_key(vk):
    _user32.keybd_event(vk, 0, 0, 0)
    _user32.keybd_event(vk, 0, KEYEVENTF_KEYUP, 0)

def volume_up(step=0):
    _volume_key(VK_VOLUME_UP)

def volume_down(step=0):
    _volume_key(VK_VOLUME_DOWN)

class MediaPoller(QObject):
    data_updated = pyqtSignal(dict)

    def __init__(self):
        super().__init__()
        self.manager = None
        self.running = True
        self.thread = threading.Thread(target=self._update_loop, daemon=True)
        self.thread.start()

    def _update_loop(self):
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        loop.run_until_complete(self._poll_async())

    async def _poll_async(self):
        while self.running:
            try:
                if not self.manager:
                    self.manager = await SessionManager.request_async()
                
                session = self.manager.get_current_session()
                
                # Actively look for a playing session to override the default if needed
                try:
                    sessions = self.manager.get_sessions()
                    if sessions:
                        for s in sessions:
                            pb = s.get_playback_info()
                            if pb and int(pb.playback_status) == 4:
                                session = s
                                break
                except Exception:
                    pass

                if not session:
                    self.data_updated.emit({"title": "NO MEDIA DETECTED", "artist": "", "status": 0, "image_data": None})
                else:
                    props = await session.try_get_media_properties_async()
                    playback = session.get_playback_info()
                    
                    image_data = None
                    if props.thumbnail:
                        try:
                            stream = await props.thumbnail.open_read_async()
                            reader = DataReader(stream.get_input_stream_at(0))
                            await reader.load_async(stream.size)
                            buffer = reader.read_buffer(stream.size)
                            
                            data = (ctypes.c_ubyte * stream.size)()
                            ctypes.memmove(data, ctypes.addressof(ctypes.c_char.from_buffer(buffer)), stream.size)
                            image_data = bytes(data)
                        except:
                            pass

                    self.data_updated.emit({
                        "title": (props.title or "NO MEDIA DETECTED").upper(),
                        "artist": (props.artist or "").upper(),
                        "status": 1 if int(playback.playback_status) == 4 else 0,
                        "image_data": image_data
                    })
            except Exception as e:
                pass
            
            time.sleep(0.5)

    def control(self, action):
        async def _control_async():
            try:
                if not self.manager:
                    self.manager = await SessionManager.request_async()
                session = self.manager.get_current_session()
                
                # Apply control to the actively playing session, or default to the last active
                try:
                    sessions = self.manager.get_sessions()
                    if sessions:
                        for s in sessions:
                            pb = s.get_playback_info()
                            if pb and int(pb.playback_status) == 4:
                                session = s
                                break
                except Exception:
                    pass

                if session:
                    if action == "next": await session.try_skip_next_async()
                    elif action == "prev": await session.try_skip_previous_async()
                    elif action == "toggle": await session.try_toggle_play_pause_async()
            except:
                pass
                
        def run_it():
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            loop.run_until_complete(_control_async())
            
        threading.Thread(target=run_it, daemon=True).start()


class AlbumArt(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedSize(110, 110)
        self.pixmap = None
        self.angle = 0.0
        self.is_playing = False

        self.anim_timer = QTimer(self)
        self.anim_timer.timeout.connect(self.update_rotation)
        self.anim_timer.setInterval(16)

    def set_image(self, data):
        if data:
            pix = QPixmap()
            pix.loadFromData(data)
            self.pixmap = pix
        else:
            self.pixmap = None
        self.update()

    def set_playing(self, playing):
        self.is_playing = playing
        if playing and not self.anim_timer.isActive():
            self.anim_timer.start()
        elif not playing and self.anim_timer.isActive():
            self.anim_timer.stop()

    def update_rotation(self):
        self.angle += 0.72
        if self.angle >= 360:
            self.angle -= 360
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform)

        rect = self.rect()
        center = rect.center()

        painter.setBrush(QColor('#111111'))
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawEllipse(rect)

        if self.pixmap and not self.pixmap.isNull():
            path = QPainterPath()
            path.addEllipse(QRectF(rect))
            painter.setClipPath(path)

            painter.translate(center)
            painter.rotate(self.angle)
            painter.translate(-center)

            scaled_pix = self.pixmap.scaled(rect.size(), Qt.AspectRatioMode.KeepAspectRatioByExpanding, Qt.TransformationMode.SmoothTransformation)
            
            x = (rect.width() - scaled_pix.width()) // 2
            y = (rect.height() - scaled_pix.height()) // 2
            painter.drawPixmap(x, y, scaled_pix)
            
            painter.resetTransform()
            painter.setClipping(False)

        pen = QPen(QColor('#b84d26'))
        pen.setWidth(2)
        painter.setPen(pen)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        border_rect = rect.adjusted(1, 1, -1, -1)
        painter.drawEllipse(border_rect)

    def wheelEvent(self, event):
        center = self.rect().center()
        pos = event.position()
        dx = pos.x() - center.x()
        dy = pos.y() - center.y()
        radius = self.width() / 2
        if dx * dx + dy * dy <= radius * radius:
            if event.angleDelta().y() > 0:
                volume_up()
            else:
                volume_down()


class ControlButton(QWidget):
    clicked = pyqtSignal()

    def __init__(self, icon_type, parent=None):
        super().__init__(parent)
        self.icon_type = icon_type
        self.setFixedSize(24, 24)
        self.hovered = False
        self.pressed = False
        self.scale = 1.0
        self.nudge_offset = 0

        self.ripple_radius = 0
        self.ripple_opacity = 0.0
        self.anim_timer = QTimer(self)
        self.anim_timer.timeout.connect(self._animate_ripple)
        self.anim_timer.setInterval(16)

        self.pop_timer = QTimer(self)
        self.pop_timer.setSingleShot(True)
        self.pop_timer.timeout.connect(self._pop_back)

        self.nudge_timer = QTimer(self)
        self.nudge_timer.timeout.connect(self._animate_nudge)
        self.nudge_timer.setInterval(16)

    def _pop_back(self):
        self.scale = 1.0
        self.update()

    def nudge(self, direction):
        self.nudge_offset = 5 * direction
        self.nudge_timer.start()

    def _animate_nudge(self):
        self.nudge_offset *= 0.7
        if abs(self.nudge_offset) < 0.3:
            self.nudge_offset = 0
            self.nudge_timer.stop()
        self.update()

    def _animate_ripple(self):
        self.ripple_radius += 1.5
        self.ripple_opacity -= 0.05
        if self.ripple_opacity <= 0:
            self.anim_timer.stop()
        self.update()

    def enterEvent(self, event):
        self.hovered = True
        self.update()

    def leaveEvent(self, event):
        self.hovered = False
        self.pressed = False
        self.update()

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self.pressed = True
            self.scale = 0.85
            self.pop_timer.start(80)
            self.ripple_radius = 0
            self.ripple_opacity = 0.5
            self.anim_timer.start()
            self.update()

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton and self.pressed:
            self.pressed = False
            self.clicked.emit()
            if self.icon_type == 'next':
                self.nudge(1)
            elif self.icon_type == 'prev':
                self.nudge(-1)
            self.update()

    def set_icon(self, icon_type):
        self.icon_type = icon_type
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        if self.anim_timer.isActive():
            painter.setBrush(QColor(255, 255, 255, int(255 * max(0, self.ripple_opacity))))
            painter.setPen(Qt.PenStyle.NoPen)
            center = self.rect().center()
            painter.drawEllipse(center, int(self.ripple_radius), int(self.ripple_radius))

        color = QColor('#ff6a3d') if self.hovered else QColor('#b84d26')
        if self.pressed:
            color = color.darker(120)

        painter.setBrush(color)
        painter.setPen(Qt.PenStyle.NoPen)

        # scale around center + nudge
        painter.translate(self.width()/2, self.height()/2)
        painter.scale(self.scale, self.scale)
        painter.translate(-self.width()/2, -self.height()/2)
        painter.translate(self.nudge_offset, 0)

        rect = self.rect()
        w, h = rect.width(), rect.height()

        if self.icon_type == 'play':
            poly = QPolygonF([QPointF(w/5, h/4), QPointF(w/5, 3*h/4), QPointF(4*w/5, h/2)])
            painter.drawPolygon(poly)
        elif self.icon_type == 'pause':
            # Two clean bars, more refined spacing
            painter.drawRect(int(7*w//24), int(h//4), int(4*w//24), int(h//2))
            painter.drawRect(int(13*w//24), int(h//4), int(4*w//24), int(h//2))
        elif self.icon_type == 'prev':
            # Bar on left, triangle pointing left on right (mirror of next)
            painter.drawRect(int(w/6), int(h/4), int(w/6), int(h/2))
            poly = QPolygonF([QPointF(5*w/6, h/4), QPointF(5*w/6, 3*h/4), QPointF(w/2, h/2)])
            painter.drawPolygon(poly)
        elif self.icon_type == 'next':
            # Triangle pointing right on left, bar on right
            poly = QPolygonF([QPointF(w/6, h/4), QPointF(w/6, 3*h/4), QPointF(w/2, h/2)])
            painter.drawPolygon(poly)
            painter.drawRect(int(2*w/3), int(h/4), int(w/6), int(h/2))


class MediaPlayerWidget(QMainWindow):
    def __init__(self):
        super().__init__()
        
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground, True)
        self.setFixedSize(140, 200)

        from PyQt6.QtWidgets import QFrame
        self.main_widget = QFrame(self)
        self.main_widget.setObjectName("player")
        self.main_widget.setStyleSheet("""
            QFrame#player {
                background-color: #1e1c19;
                border-radius: 0px;
            }
        """)
        self.setCentralWidget(self.main_widget)

        # Drop shadow
        shadow = QGraphicsDropShadowEffect()
        shadow.setBlurRadius(20)
        shadow.setOffset(0, 4)
        shadow.setColor(QColor(0, 0, 0, 140))
        self.main_widget.setGraphicsEffect(shadow)

        layout = QVBoxLayout(self.main_widget)
        layout.setContentsMargins(15, 15, 15, 15)
        layout.setSpacing(10)
        layout.setAlignment(Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignHCenter)

        self.album_art = AlbumArt(self)
        layout.addWidget(self.album_art, alignment=Qt.AlignmentFlag.AlignHCenter)

        self.title_label = QLabel("NO MEDIA DETECTED")
        self.title_label.setAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter)
        self.title_label.setStyleSheet("""
            color: #c8c3bc;
            font-size: 10px;
            font-weight: bold;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
            background: transparent;
        """)
        layout.addWidget(self.title_label)

        controls_layout = QHBoxLayout()
        controls_layout.setSpacing(15)
        controls_layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        
        self.btn_prev = ControlButton('prev', self)
        self.btn_playpause = ControlButton('play', self)
        self.btn_next = ControlButton('next', self)

        controls_layout.addWidget(self.btn_prev)
        controls_layout.addWidget(self.btn_playpause)
        controls_layout.addWidget(self.btn_next)

        layout.addLayout(controls_layout)

        self._drag_pos = None

        self.poller = MediaPoller()
        self.poller.data_updated.connect(self.update_ui)
        
        self.btn_prev.clicked.connect(lambda: self.poller.control('prev'))
        self.btn_playpause.clicked.connect(self.toggle_play_pause)
        self.btn_next.clicked.connect(lambda: self.poller.control('next'))

        self.setup_tray()
        
        self.last_image_data = None

    def toggle_play_pause(self):
        # Visually toggle immediately for responsiveness
        new_icon = 'pause' if self.btn_playpause.icon_type == 'play' else 'play'
        self.btn_playpause.set_icon(new_icon)
        self.poller.control('toggle')

    def setup_tray(self):
        self.tray_icon = QSystemTrayIcon(self)
        self.tray_icon.setIcon(QIcon(resource_path('icon.ico')))
        
        menu = QMenu()
        close_action = QAction("Close Player", self)
        close_action.triggered.connect(QApplication.quit)
        menu.addAction(close_action)
        
        self.tray_icon.setContextMenu(menu)
        self.tray_icon.show()

    def update_ui(self, data):
        self.title_label.setText(data['title'])
        
        if data['image_data'] != self.last_image_data:
            self.last_image_data = data['image_data']
            self.album_art.set_image(self.last_image_data)

        is_playing = (data['status'] == 1)
        self.album_art.set_playing(is_playing)
        self.btn_playpause.set_icon('pause' if is_playing else 'play')

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self._drag_pos = event.globalPosition().toPoint()

    def mouseMoveEvent(self, event):
        if self._drag_pos is not None:
            delta = event.globalPosition().toPoint() - self._drag_pos
            self.move(self.pos() + delta)
            self._drag_pos = event.globalPosition().toPoint()

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self._drag_pos = None

if __name__ == "__main__":
    # Single-instance lock via named mutex
    mutex = ctypes.windll.kernel32.CreateMutexW(None, False, "IAMMUSIC_SingleInstance")
    if ctypes.windll.kernel32.GetLastError() == 183:  # ERROR_ALREADY_EXISTS
        ctypes.windll.kernel32.CloseHandle(mutex)
        sys.exit(0)

    app = QApplication(sys.argv)
    app.setWindowIcon(QIcon(resource_path('icon.ico')))
    
    player = MediaPlayerWidget()
    player.show()
    
    exit_code = app.exec()
    ctypes.windll.kernel32.ReleaseMutex(mutex)
    ctypes.windll.kernel32.CloseHandle(mutex)
    sys.exit(exit_code)
