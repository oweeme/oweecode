// Starter games for desktop toolkits: the same tiny "dodge the blocks" game in PySide6,
// QML, GTK4 and pygame, so a game project starts from something that already runs.
export interface GameTemplate { id: string; label: string; file: string; run: string; code: string }

const PYSIDE = `import random
import sys

from PySide6.QtCore import QRectF, Qt, QTimer
from PySide6.QtGui import QColor, QFont, QPainter
from PySide6.QtWidgets import QApplication, QWidget

W, H = 480, 640


class Game(QWidget):
    """Esquivá los bloques: flechas o A/D para moverte, ESPACIO para reiniciar."""

    def __init__(self):
        super().__init__()
        self.setFixedSize(W, H)
        self.setWindowTitle("Esquiva (PySide6)")
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self.keys = set()
        self.reset()
        self.timer = QTimer(self)          # el "game loop": ~60 cuadros por segundo
        self.timer.timeout.connect(self.tick)
        self.timer.start(16)

    def reset(self):
        self.px, self.obstacles, self.score, self.over, self.ticks = W / 2 - 25, [], 0, False, 0

    def keyPressEvent(self, e):
        self.keys.add(e.key())
        if e.key() == Qt.Key.Key_Space and self.over:
            self.reset()

    def keyReleaseEvent(self, e):
        self.keys.discard(e.key())

    def tick(self):
        if not self.over:
            right = Qt.Key.Key_Right in self.keys or Qt.Key.Key_D in self.keys
            left = Qt.Key.Key_Left in self.keys or Qt.Key.Key_A in self.keys
            self.px = max(0, min(W - 50, self.px + (right - left) * 6))
            self.ticks += 1
            if self.ticks % max(10, 40 - self.score // 300) == 0:
                self.obstacles.append([random.randint(0, W - 40), -40.0, random.uniform(3, 6)])
            for o in self.obstacles:
                o[1] += o[2]
            self.obstacles = [o for o in self.obstacles if o[1] < H]
            self.score += 1
            player = QRectF(self.px, H - 60, 50, 30)
            if any(player.intersects(QRectF(o[0], o[1], 40, 40)) for o in self.obstacles):
                self.over = True
        self.update()

    def paintEvent(self, _):
        p = QPainter(self)
        p.fillRect(self.rect(), QColor("#101820"))
        p.fillRect(QRectF(self.px, H - 60, 50, 30), QColor("#4cc9f0"))
        for o in self.obstacles:
            p.fillRect(QRectF(o[0], o[1], 40, 40), QColor("#f72585"))
        p.setPen(QColor("white"))
        p.setFont(QFont("sans", 14))
        p.drawText(12, 28, f"Puntos: {self.score // 10}")
        if self.over:
            p.setFont(QFont("sans", 26))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "Game over\\nESPACIO para reiniciar")


if __name__ == "__main__":
    app = QApplication(sys.argv)
    game = Game()
    game.show()
    sys.exit(app.exec())
`

const QML = `import QtQuick
import QtQuick.Window

// Esquivá los bloques: flechas o A/D para moverte, ESPACIO para reiniciar.
Window {
    id: win
    width: 480
    height: 640
    visible: true
    title: "Esquiva (QML)"
    color: "#101820"

    property int score: 0
    property bool over: false
    property real dir: 0
    property int ticks: 0

    function restart() { obstacles.clear(); score = 0; ticks = 0; over = false; player.x = width / 2 - 25 }

    function tick() {
        player.x = Math.max(0, Math.min(width - player.width, player.x + dir * 6))
        ticks++
        if (ticks % Math.max(10, 40 - Math.floor(score / 300)) === 0)
            obstacles.append({ ox: Math.random() * (width - 40), oy: -40, speed: 3 + Math.random() * 3 })
        for (var i = obstacles.count - 1; i >= 0; i--) {
            var o = obstacles.get(i), ny = o.oy + o.speed
            if (ny > height) { obstacles.remove(i); continue }
            obstacles.setProperty(i, "oy", ny)
            if (o.ox < player.x + player.width && o.ox + 40 > player.x && ny < player.y + player.height && ny + 40 > player.y) over = true
        }
        score++
    }

    ListModel { id: obstacles }

    Item {
        anchors.fill: parent
        focus: true
        Keys.onPressed: (e) => {
            if (e.key === Qt.Key_Left || e.key === Qt.Key_A) win.dir = -1
            else if (e.key === Qt.Key_Right || e.key === Qt.Key_D) win.dir = 1
            else if (e.key === Qt.Key_Space && win.over) win.restart()
        }
        Keys.onReleased: (e) => { if (!e.isAutoRepeat) win.dir = 0 }

        Rectangle { id: player; width: 50; height: 30; y: parent.height - 60; x: 215; radius: 6; color: "#4cc9f0" }
        Repeater {
            model: obstacles
            Rectangle { x: model.ox; y: model.oy; width: 40; height: 40; color: "#f72585" }
        }
        Text { x: 12; y: 8; color: "white"; font.pixelSize: 20; text: "Puntos: " + Math.floor(win.score / 10) }
        Text { visible: win.over; anchors.centerIn: parent; color: "white"; font.pixelSize: 28; horizontalAlignment: Text.AlignHCenter; text: "Game over\\nESPACIO para reiniciar" }
    }

    Timer { interval: 16; running: !win.over; repeat: true; onTriggered: win.tick() }
}
`

// Requiere: sudo apt install python3-gi python3-gi-cairo gir1.2-gtk-4.0
const GTK = `# Requiere (Ubuntu/Debian): sudo apt install python3-gi python3-gi-cairo gir1.2-gtk-4.0
import random
import sys

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gdk, GLib, Gtk  # noqa: E402

W, H = 480, 640


class Game(Gtk.DrawingArea):
    """Esquivá los bloques: flechas o A/D para moverte, ESPACIO para reiniciar."""

    def __init__(self):
        super().__init__()
        self.set_content_width(W)
        self.set_content_height(H)
        self.set_draw_func(self.draw)
        self.keys = set()
        self.reset()
        GLib.timeout_add(16, self.tick)          # el "game loop": ~60 cuadros por segundo

    def reset(self):
        self.px, self.obstacles, self.score, self.over, self.ticks = W / 2 - 25, [], 0, False, 0

    def on_press(self, _ctrl, keyval, _code, _state):
        self.keys.add(keyval)
        if keyval == Gdk.KEY_space and self.over:
            self.reset()
        return True

    def on_release(self, _ctrl, keyval, _code, _state):
        self.keys.discard(keyval)

    def tick(self):
        if not self.over:
            right = Gdk.KEY_Right in self.keys or Gdk.KEY_d in self.keys
            left = Gdk.KEY_Left in self.keys or Gdk.KEY_a in self.keys
            self.px = max(0, min(W - 50, self.px + (right - left) * 6))
            self.ticks += 1
            if self.ticks % max(10, 40 - self.score // 300) == 0:
                self.obstacles.append([random.randint(0, W - 40), -40.0, random.uniform(3, 6)])
            for o in self.obstacles:
                o[1] += o[2]
            self.obstacles = [o for o in self.obstacles if o[1] < H]
            self.score += 1
            if any(self.px < o[0] + 40 and self.px + 50 > o[0] and H - 60 < o[1] + 40 and H - 30 > o[1] for o in self.obstacles):
                self.over = True
        self.queue_draw()
        return True                                # True = seguir llamando

    def draw(self, _area, cr, width, height):
        cr.set_source_rgb(0.06, 0.09, 0.13)
        cr.paint()
        cr.set_source_rgb(0.3, 0.79, 0.94)
        cr.rectangle(self.px, H - 60, 50, 30)
        cr.fill()
        cr.set_source_rgb(0.97, 0.15, 0.52)
        for o in self.obstacles:
            cr.rectangle(o[0], o[1], 40, 40)
            cr.fill()
        cr.set_source_rgb(1, 1, 1)
        cr.set_font_size(18)
        cr.move_to(12, 28)
        cr.show_text(f"Puntos: {self.score // 10}")
        if self.over:
            cr.set_font_size(26)
            cr.move_to(width / 2 - 70, height / 2)
            cr.show_text("Game over")
            cr.set_font_size(16)
            cr.move_to(width / 2 - 80, height / 2 + 32)
            cr.show_text("ESPACIO para reiniciar")


class App(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="com.example.esquiva")

    def do_activate(self):
        win = Gtk.ApplicationWindow(application=self, title="Esquiva (GTK4)")
        game = Game()
        win.set_child(game)
        keys = Gtk.EventControllerKey()
        keys.connect("key-pressed", game.on_press)
        keys.connect("key-released", game.on_release)
        win.add_controller(keys)
        win.present()


if __name__ == "__main__":
    sys.exit(App().run(sys.argv))
`

const PYGAME = `import random
import sys

import pygame   # se instala en el entorno del proyecto: python3 -m venv .venv && .venv/bin/pip install pygame

W, H = 480, 640
pygame.init()
screen = pygame.display.set_mode((W, H))
pygame.display.set_caption("Esquiva (pygame)")
clock = pygame.time.Clock()
font = pygame.font.SysFont("sans", 24)


def new_game():
    return {"x": W / 2 - 25, "obs": [], "score": 0, "over": False, "ticks": 0}


g = new_game()
while True:
    for e in pygame.event.get():
        if e.type == pygame.QUIT:
            pygame.quit()
            sys.exit()
        if e.type == pygame.KEYDOWN and e.key == pygame.K_SPACE and g["over"]:
            g = new_game()

    if not g["over"]:
        keys = pygame.key.get_pressed()
        dx = (keys[pygame.K_RIGHT] or keys[pygame.K_d]) - (keys[pygame.K_LEFT] or keys[pygame.K_a])
        g["x"] = max(0, min(W - 50, g["x"] + dx * 6))
        g["ticks"] += 1
        if g["ticks"] % max(10, 40 - g["score"] // 300) == 0:
            g["obs"].append([random.randint(0, W - 40), -40.0, random.uniform(3, 6)])
        for o in g["obs"]:
            o[1] += o[2]
        g["obs"] = [o for o in g["obs"] if o[1] < H]
        g["score"] += 1
        player = pygame.Rect(g["x"], H - 60, 50, 30)
        if any(player.colliderect(pygame.Rect(o[0], o[1], 40, 40)) for o in g["obs"]):
            g["over"] = True

    screen.fill((16, 24, 32))
    pygame.draw.rect(screen, (76, 201, 240), (g["x"], H - 60, 50, 30), border_radius=6)
    for o in g["obs"]:
        pygame.draw.rect(screen, (247, 37, 133), (o[0], o[1], 40, 40))
    screen.blit(font.render(f"Puntos: {g['score'] // 10}", True, (255, 255, 255)), (12, 8))
    if g["over"]:
        screen.blit(font.render("Game over - ESPACIO para reiniciar", True, (255, 255, 255)), (60, H / 2))
    pygame.display.flip()
    clock.tick(60)
`

export const GAME_TEMPLATES: GameTemplate[] = [
  { id: 'pyside', label: 'Juego 2D — Python + Qt (PySide6, QPainter)', file: 'juego_qt.py', run: 'python3 juego_qt.py', code: PYSIDE },
  { id: 'qml', label: 'Juego 2D — QML (Qt Quick)', file: 'juego.qml', run: 'qml6 juego.qml', code: QML },
  { id: 'gtk', label: 'Juego 2D — Python + GTK4 (Cairo)', file: 'juego_gtk.py', run: 'python3 juego_gtk.py', code: GTK },
  { id: 'pygame', label: 'Juego 2D — pygame', file: 'juego_pygame.py', run: 'python3 -m venv .venv && .venv/bin/pip install -q pygame && .venv/bin/python juego_pygame.py', code: PYGAME },
]
