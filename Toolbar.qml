import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15

Rectangle {
    id: root
    width: row.implicitWidth
    height: 36
    color: "white"
    border.color: "black"
    border.width: 1

    signal cancelRequested()
    signal saveRequested()
    signal confirmRequested()

    RowLayout {
        id: row
        anchors.fill: parent
        anchors.margins: 1
        spacing: 0

        // 1. Close Button (X)
        Button {
            id: closeBtn
            Layout.preferredWidth: 32
            Layout.preferredHeight: 32
            padding: 0
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
        }

        // 2. Save Button (Floppy)
        Button {
            id: saveBtn
            Layout.preferredWidth: 32
            Layout.preferredHeight: 32
            padding: 0
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
                    ctx.beginPath();
                    // Floppy disk outline
                    ctx.rect(5, 4, 14, 16);
                    ctx.stroke();
                    
                    ctx.beginPath();
                    // Top notch
                    ctx.moveTo(15, 4);
                    ctx.lineTo(15, 8);
                    ctx.lineTo(19, 8);
                    ctx.lineTo(19, 20); // Continue right side down
                    ctx.stroke();

                    // Actually rect above draws outline but we need to cover the lines.
                    // The Canvas path drawing is simple, let's just draw distinct lines for clarity like the python code.
                    // Python: rect(5,4, 14,16) -> x=5,y=4, w=14, h=16
                    
                    // Reset and draw line by line to match exactly if needed, or use rects.
                    // Let's stick effectively to the visual intent.
                    
                    ctx.reset();
                    ctx.strokeStyle = "black";
                    ctx.lineWidth = 2;
                    
                    // Main box
                    ctx.strokeRect(5, 4, 14, 16);
                    
                    // Top notch detail (mimicking python code logic approx)
                    // The python code drew lines: (15,4)->(15,8)->(19,8)->(19,20)
                    // (19,8)->(19,20) overlaps the right side of the rect.
                    // (15,4)->(15,8) is internal line? No, 15 is x. 5+14=19 is right edge. 
                    // So it's drawing the inner cutout for the shutter? 
                    // Let's just draw the standard floppy look.
                    
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
        }

        // 3. Confirm Button (Checkmark)
        Button {
            id: confirmBtn
            Layout.preferredWidth: 32
            Layout.preferredHeight: 32
            padding: 0
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
        }
    }
}
