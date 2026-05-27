import QtQuick 2.15
import AiosPrimitives 1.0

Item {
    id: indicator
    width: 240
    height: 40

    property alias subject: backend.subject
    property alias evidenceLink: backend.evidenceLink

    Rectangle {
        anchors.fill: parent
        radius: 4
        color: "#1a1a2e"
        border.color: "#4a4a6a"
        border.width: 1

        Row {
            anchors.centerIn: parent
            spacing: 8

            Text {
                text: backend.subject || "—"
                color: "#c0c0e0"
                font.pixelSize: 12
            }

            Rectangle {
                visible: backend.evidenceLink !== ""
                width: 8
                height: 8
                radius: 4
                color: "#00cc66"
                anchors.verticalCenter: parent.verticalCenter
            }

            Text {
                text: backend.evidenceLink ? "E+" : ""
                color: "#00cc66"
                font.pixelSize: 10
                font.bold: true
                visible: backend.evidenceLink !== ""
            }
        }
    }

    AiosApprovalPrompt {
        id: backend
    }
}
