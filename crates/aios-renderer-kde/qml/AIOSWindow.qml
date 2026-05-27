import QtQuick 2.15
import QtQuick.Controls 2.15
import AiosPrimitives 1.0

ApplicationWindow {
    id: aiosWindow
    title: backend.title
    visible: true
    width: 1280
    height: 800

    // ── Recovery banner overlay (INV I2) ──
    Rectangle {
        visible: backend.recoveryActive
        anchors.top: parent.top
        width: parent.width
        height: 32
        color: "#b35900"
        Text {
            anchors.centerIn: parent
            text: "AIOS RECOVERY MODE"
            color: "white"
            font.pixelSize: 14
        }
    }

    // Backend object — QObject C++ bridge from Rust
    AiosWindow {
        id: backend
    }
}
