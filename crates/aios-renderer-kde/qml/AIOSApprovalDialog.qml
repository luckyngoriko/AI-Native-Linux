import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import AiosPrimitives 1.0

Dialog {
    id: approvalDialog
    title: backend.subject || "AIOS Approval Required"
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    width: 480
    height: 320

    ColumnLayout {
        anchors.fill: parent
        spacing: 12

        Text {
            text: backend.subject
            font.bold: true
            font.pixelSize: 16
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }

        Text {
            text: backend.actionSummary
            font.pixelSize: 13
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }

        Text {
            text: backend.evidenceLink ? "Evidence: " + backend.evidenceLink : ""
            font.pixelSize: 11
            color: "#666666"
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignRight

            Button {
                text: "Reject"
                onClicked: backend.decided(false)
            }
            Button {
                text: "Approve"
                onClicked: backend.decided(true)
            }
        }
    }

    onAccepted: backend.decided(true)
    onRejected: backend.decided(false)

    AiosApprovalPrompt {
        id: backend
    }
}
