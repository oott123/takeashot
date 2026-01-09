import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import QtQuick.Shapes 1.15

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
    signal toolSelected(string toolName)

    function selectTool(name) {
        if (name === "pointer") pointerBtn.checked = true
        else if (name === "pencil") pencilBtn.checked = true
        else if (name === "line") lineBtn.checked = true
        else if (name === "rect") rectBtn.checked = true
        else if (name === "ellipse") ellipseBtn.checked = true
    }

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

            // Tool Group
            ButtonGroup {
                id: toolGroup
                buttons: [pointerBtn, pencilBtn, lineBtn, rectBtn, ellipseBtn]
            }

            // --- Tools ---

            // Pointer
            Button {
                id: pointerBtn
                Layout.preferredWidth: 32
                Layout.preferredHeight: 32
                padding: 4
                checkable: true
                checked: true
                onToggled: if(checked) root.toolSelected("pointer")
                background: Rectangle {
                    color: pointerBtn.hovered ? "#eee" : (pointerBtn.checked ? "#ddd" : "transparent")
                    border.color: pointerBtn.checked ? "#aaa" : "transparent"
                }
                contentItem: Shape {
                    anchors.centerIn: parent
                    width: 24; height: 24
                    ShapePath {
                        strokeColor: "black"
                        strokeWidth: 2
                        fillColor: "transparent"
                        capStyle: ShapePath.RoundCap
                        joinStyle: ShapePath.RoundJoin
                        PathSvg { path: "M7 7l10 10 M16 7l-9 0l0 9" }
                    }
                }
                ToolTip.visible: hovered
                ToolTip.text: "Select / Edit"
            }

            // Pencil
            Button {
                id: pencilBtn
                Layout.preferredWidth: 32
                Layout.preferredHeight: 32
                padding: 4
                checkable: true
                onToggled: if(checked) root.toolSelected("pencil")
                background: Rectangle {
                    color: pencilBtn.hovered ? "#eee" : (pencilBtn.checked ? "#ddd" : "transparent")
                    border.color: pencilBtn.checked ? "#aaa" : "transparent"
                }
                contentItem: Shape {
                    anchors.centerIn: parent
                    width: 24; height: 24
                    ShapePath {
                        strokeColor: "black"
                        strokeWidth: 2
                        fillColor: "transparent"
                        capStyle: ShapePath.RoundCap
                        joinStyle: ShapePath.RoundJoin
                        PathSvg { path: "M4 20h4l10.5 -10.5a2.828 2.828 0 1 0 -4 -4l-10.5 10.5v4 M13.5 6.5l4 4" }
                    }
                }
                ToolTip.visible: hovered
                ToolTip.text: "Pencil"
            }

            // Line
            Button {
                id: lineBtn
                Layout.preferredWidth: 32
                Layout.preferredHeight: 32
                padding: 4
                checkable: true
                onToggled: if(checked) root.toolSelected("line")
                background: Rectangle {
                    color: lineBtn.hovered ? "#eee" : (lineBtn.checked ? "#ddd" : "transparent")
                    border.color: lineBtn.checked ? "#aaa" : "transparent"
                }
                contentItem: Shape {
                    anchors.centerIn: parent
                    width: 24; height: 24
                    ShapePath {
                        strokeColor: "black"
                        strokeWidth: 2
                        fillColor: "transparent"
                        capStyle: ShapePath.RoundCap
                        joinStyle: ShapePath.RoundJoin
                        PathSvg { path: "M17 5l-10 14" }
                    }
                }
                ToolTip.visible: hovered
                ToolTip.text: "Line"
            }

            // Rectangle
            Button {
                id: rectBtn
                Layout.preferredWidth: 32
                Layout.preferredHeight: 32
                padding: 4
                checkable: true
                onToggled: if(checked) root.toolSelected("rect")
                background: Rectangle {
                    color: rectBtn.hovered ? "#eee" : (rectBtn.checked ? "#ddd" : "transparent")
                    border.color: rectBtn.checked ? "#aaa" : "transparent"
                }
                contentItem: Shape {
                    anchors.centerIn: parent
                    width: 24; height: 24
                    ShapePath {
                        strokeColor: "black"
                        strokeWidth: 2
                        fillColor: "transparent"
                        capStyle: ShapePath.RoundCap
                        joinStyle: ShapePath.RoundJoin
                        PathSvg { path: "M3 7a2 2 0 0 1 2 -2h14a2 2 0 0 1 2 2v10a2 2 0 0 1 -2 2h-14a2 2 0 0 1 -2 -2v-10" }
                    }
                }
                ToolTip.visible: hovered
                ToolTip.text: "Rectangle"
            }

            // Ellipse
            Button {
                id: ellipseBtn
                Layout.preferredWidth: 32
                Layout.preferredHeight: 32
                padding: 4
                checkable: true
                onToggled: if(checked) root.toolSelected("ellipse")
                background: Rectangle {
                    color: ellipseBtn.hovered ? "#eee" : (ellipseBtn.checked ? "#ddd" : "transparent")
                    border.color: ellipseBtn.checked ? "#aaa" : "transparent"
                }
                contentItem: Shape {
                    anchors.centerIn: parent
                    width: 24; height: 24
                    ShapePath {
                        strokeColor: "black"
                        strokeWidth: 2
                        fillColor: "transparent"
                        capStyle: ShapePath.RoundCap
                        joinStyle: ShapePath.RoundJoin
                        PathSvg { path: "M3 12a9 9 0 1 0 18 0a9 9 0 1 0 -18 0" }
                    }
                }
                ToolTip.visible: hovered
                ToolTip.text: "Ellipse"
            }

            // Separator
            Rectangle {
                Layout.preferredWidth: 1
                Layout.preferredHeight: 20
                color: "#ccc"
                Layout.leftMargin: 4
                Layout.rightMargin: 4
            }

            // 1. Close Button (X)
            Button {
                id: closeBtn
                Layout.preferredWidth: 32
                Layout.preferredHeight: 32
                padding: 4
                background: Rectangle {
                    color: closeBtn.hovered ? "#eee" : "transparent"
                }
                contentItem: Shape {
                    anchors.centerIn: parent
                    width: 24; height: 24
                    ShapePath {
                        strokeColor: "black"
                        strokeWidth: 2
                        fillColor: "transparent"
                        capStyle: ShapePath.RoundCap
                        joinStyle: ShapePath.RoundJoin
                        PathSvg { path: "M18 6l-12 12 M6 6l12 12" }
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
                contentItem: Shape {
                    anchors.centerIn: parent
                    width: 24; height: 24
                    ShapePath {
                        strokeColor: "black"
                        strokeWidth: 2
                        fillColor: "transparent"
                        capStyle: ShapePath.RoundCap
                        joinStyle: ShapePath.RoundJoin
                        PathSvg { path: "M6 4h10l4 4v10a2 2 0 0 1 -2 2h-12a2 2 0 0 1 -2 -2v-12a2 2 0 0 1 2 -2 M10 14a2 2 0 1 0 4 0a2 2 0 1 0 -4 0 M14 4l0 4l-6 0l0 -4" }
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
                contentItem: Shape {
                    anchors.centerIn: parent
                    width: 24; height: 24
                    ShapePath {
                        strokeColor: "black"
                        strokeWidth: 2
                        fillColor: "transparent"
                        capStyle: ShapePath.RoundCap
                        joinStyle: ShapePath.RoundJoin
                        PathSvg { path: "M5 12l5 5l10 -10" }
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
