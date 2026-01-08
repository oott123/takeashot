import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15

Item {
    id: root
    // Width matches the visual toolbar
    width: toolbarRect.width
    // Height includes the visual toolbar + top padding for tooltips
    height: toolbarRect.height + topPadding
    
    // Transparent padding at the top for tooltips
    property int topPadding: 40

    signal cancelRequested()
    signal saveRequested()
    signal confirmRequested()

    // The actual visible toolbar
    Rectangle {
        id: toolbarRect
        anchors.bottom: parent.bottom
        width: row.implicitWidth + 2 // +2 for borders
        height: row.implicitHeight + 2 // +2 for borders
        color: "white"
        border.color: "black"
        border.width: 1

        RowLayout {
            id: row
            anchors.centerIn: parent
            spacing: 0

            // 1. Close Button (X)
            Button {
                id: closeBtn
                Layout.preferredWidth: 32
                Layout.preferredHeight: 32
                padding: 4
                background: Rectangle {
                    color: closeBtn.hovered ? "#eee" : "transparent"
                }
                contentItem: Canvas {
                    anchors.centerIn: parent
                    width: 24
                    height: 24
                    onPaint: {
                        var ctx = getContext("2d");
                        ctx.reset();
                        ctx.strokeStyle = "black";
                        ctx.lineWidth = 2;
                        ctx.beginPath();
                        ctx.moveTo(6, 6);
                        ctx.lineTo(18, 18);
                        ctx.moveTo(18, 6);
                        ctx.lineTo(6, 18);
                        ctx.stroke();
                    }
                }
                onClicked: root.cancelRequested()
                
                ToolTip.visible: hovered
                ToolTip.text: "Close"
                ToolTip.delay: 500
            }

            // 2. Save Button (Floppy)
            Button {
                id: saveBtn
                Layout.preferredWidth: 32
                Layout.preferredHeight: 32
                padding: 4
                background: Rectangle {
                    color: saveBtn.hovered ? "#eee" : "transparent"
                }
                contentItem: Canvas {
                    anchors.centerIn: parent
                    width: 24
                    height: 24
                    onPaint: {
                        var ctx = getContext("2d");
                        ctx.reset();
                        ctx.strokeStyle = "black";
                        ctx.lineWidth = 2;
                        
                        // Floppy disk outline
                        ctx.beginPath();
                        ctx.moveTo(5, 4);
                        ctx.lineTo(19, 4);
                        ctx.lineTo(19, 20);
                        ctx.lineTo(5, 20);
                        ctx.closePath();
                        ctx.stroke();
                        
                        // Top notch
                        ctx.beginPath();
                        ctx.moveTo(15, 4);
                        ctx.lineTo(15, 8);
                        ctx.lineTo(19, 8);
                        ctx.stroke();
                        
                        // Bottom save bar
                        ctx.strokeRect(7, 15, 10, 5);
                    }
                }
                onClicked: root.saveRequested()
                
                ToolTip.visible: hovered
                ToolTip.text: "Save"
                ToolTip.delay: 500
            }

            // 3. Confirm Button (Checkmark)
            Button {
                id: confirmBtn
                Layout.preferredWidth: 32
                Layout.preferredHeight: 32
                padding: 4
                background: Rectangle {
                    color: confirmBtn.hovered ? "#eee" : "transparent"
                }
                contentItem: Canvas {
                    anchors.centerIn: parent
                    width: 24
                    height: 24
                    onPaint: {
                        var ctx = getContext("2d");
                        ctx.reset();
                        ctx.strokeStyle = "black";
                        ctx.lineWidth = 2.5;
                        ctx.beginPath();
                        ctx.moveTo(6, 12);
                        ctx.lineTo(10, 17);
                        ctx.lineTo(18, 7);
                        ctx.stroke();
                    }
                }
                onClicked: root.confirmRequested()
                
                ToolTip.visible: hovered
                ToolTip.text: "Copy"
                ToolTip.delay: 500
            }
        }
    }
}
