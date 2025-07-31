import QtQuick
import QtQuick.Controls
import org.kde.kirigami as Kirigami

Item {
    id: root

    property int level: 0
    property bool charging: false
    property string label: ""
    property real size: Kirigami.Units.gridUnit * 3
    property bool inEar: true
    property bool showEarStatus: false

    width: size
    height: size + Kirigami.Units.gridUnit * 0.8 // Extra space for bottom percentage

    Item {
        id: circleContainer
        width: size
        height: size
        anchors.horizontalCenter: parent.horizontalCenter

        // Subtle shadow layers for depth
        Rectangle {
            anchors.centerIn: parent
            width: parent.width + 6
            height: parent.height + 6
            radius: width / 2
            color: Qt.rgba(0, 0, 0, 0.05)
            z: -3
        }

        Rectangle {
            anchors.centerIn: parent
            width: parent.width + 3
            height: parent.height + 3
            radius: width / 2
            color: Qt.rgba(0, 0, 0, 0.08)
            z: -2
        }

        Rectangle {
            anchors.centerIn: parent
            width: parent.width + 1
            height: parent.height + 1
            radius: width / 2
            color: Qt.rgba(0, 0, 0, 0.12)
            z: -1
        }

        // Background circle (always visible track)
        Rectangle {
            anchors.fill: parent
            radius: width / 2
            color: "transparent"
            border.width: 3
            border.color: Kirigami.ColorUtils.adjustColor(
                Kirigami.Theme.textColor,
                {"alpha": -230}
            )
        }

        // Progress circle
        Canvas {
            id: progressCanvas
            anchors.fill: parent
            rotation: -90 // Start from top

            property real progress: level / 100
            property color progressColor: {
                if (charging) return "#4CAF50"
                if (level < 20) return "#F44336"
                if (level < 50) return "#FF9800"
                return "#4CAF50"
            }

            onProgressChanged: requestPaint()
            onProgressColorChanged: requestPaint()

            onPaint: {
                var ctx = getContext("2d")
                ctx.reset()

                var centerX = width / 2
                var centerY = height / 2
                var radius = Math.min(width, height) / 2 - 2

                // Main progress arc
                ctx.beginPath()
                ctx.arc(centerX, centerY, radius, 0, 2 * Math.PI * progress)
                ctx.lineWidth = 3
                ctx.strokeStyle = progressColor
                ctx.lineCap = "round"
                ctx.stroke()

                // Charging glow effect
                if (charging && progress > 0) {
                    // Outer glow
                    ctx.globalAlpha = 0.3
                    ctx.lineWidth = 5
                    ctx.stroke()

                    // Middle glow
                    ctx.globalAlpha = 0.2
                    ctx.lineWidth = 7
                    ctx.stroke()

                    // Outer glow
                    ctx.globalAlpha = 0.1
                    ctx.lineWidth = 9
                    ctx.stroke()
                }
            }

            Behavior on progress {
                NumberAnimation {
                    duration: 800
                    easing.type: Easing.InOutCubic
                }
            }
        }

        // Center label
        Text {
            anchors.centerIn: parent
            text: label
            font.pixelSize: size * 0.38
            font.weight: Font.Medium
            color: Kirigami.Theme.textColor
            opacity: 0.9
        }

        // Charging indicator overlay
        Item {
            visible: charging
            anchors.fill: parent

            // Animated ring
            Rectangle {
                anchors.centerIn: parent
                width: parent.width * 0.85
                height: width
                radius: width / 2
                color: "transparent"
                border.width: 1
                border.color: Kirigami.ColorUtils.adjustColor(
                    progressCanvas.progressColor,
                    {"alpha": -150}
                )

                opacity: 0

                SequentialAnimation on opacity {
                    running: parent.visible
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.6; duration: 1500; easing.type: Easing.OutQuad }
                    NumberAnimation { to: 0; duration: 1500; easing.type: Easing.InQuad }
                }

                SequentialAnimation on scale {
                    running: parent.visible
                    loops: Animation.Infinite
                    NumberAnimation { from: 0.85; to: 1.15; duration: 3000; easing.type: Easing.InOutQuad }
                }
            }

            // Small charging icon
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.top: parent.top
                anchors.topMargin: size * 0.15

                width: size * 0.15
                height: width
                radius: width / 2
                color: progressCanvas.progressColor
                opacity: 0.8

                Kirigami.Icon {
                    anchors.centerIn: parent
                    source: "battery-charging-symbolic"
                    width: parent.width * 0.6
                    height: width
                    color: "white"
                }

                // Gentle pulse
                SequentialAnimation on scale {
                    running: parent.visible
                    loops: Animation.Infinite
                    NumberAnimation { to: 1.1; duration: 1000; easing.type: Easing.InOutQuad }
                    NumberAnimation { to: 1.0; duration: 1000; easing.type: Easing.InOutQuad }
                }
            }
        }
    }

    // Ear detection indicator
    Rectangle {
        visible: showEarStatus
        anchors.right: circleContainer.right
        anchors.bottom: circleContainer.bottom
        anchors.margins: -2

        width: size * 0.28
        height: width
        radius: width / 2

        color: inEar ? "#4CAF50" : "#757575"
        border.width: 2
        border.color: Kirigami.Theme.backgroundColor

        Kirigami.Icon {
            anchors.centerIn: parent
            source: inEar ? "dialog-ok" : "dialog-cancel"
            width: parent.width * 0.6
            height: width
            color: "white"
        }

        // Subtle pulse when in ear
        SequentialAnimation on scale {
            running: inEar && parent.visible
            loops: Animation.Infinite
            NumberAnimation { to: 1.1; duration: 2000; easing.type: Easing.InOutQuad }
            NumberAnimation { to: 1.0; duration: 2000; easing.type: Easing.InOutQuad }
        }
    }

    // Bottom percentage text
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: size * -0.2
        text: level + "%"
        font.pixelSize: size * 0.32
        font.weight: Font.Normal
        opacity: 0.6
        color: Kirigami.Theme.textColor
        horizontalAlignment: Text.AlignHCenter

        Behavior on color {
            ColorAnimation { duration: 300 }
        }
    }

    // Interactive hover effect
    MouseArea {
        anchors.fill: circleContainer
        hoverEnabled: true

        onEntered: {
            scaleAnimation.to = 1.05
            scaleAnimation.start()
        }

        onExited: {
            scaleAnimation.to = 1.0
            scaleAnimation.start()
        }
    }

    NumberAnimation {
        id: scaleAnimation
        target: circleContainer
        property: "scale"
        duration: 200
        easing.type: Easing.OutCubic
    }
}
