import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents3

Item {
    id: root

    property var device: null
    signal clicked()

    // Calculate average battery
    property int averageBattery: {
        if (!device || !device.battery) return 0

        var count = 0
        var total = 0

        if (device.battery.left != null) {
            total += device.battery.left
            count++
        }
        if (device.battery.right != null) {
            total += device.battery.right
            count++
        }

        return count > 0 ? Math.round(total / count) : 0
    }

    MouseArea {
        anchors.fill: parent
        onClicked: root.clicked()
        hoverEnabled: true

        // Background with gradient
        Rectangle {
            id: background
            anchors.fill: parent
            radius: width * 0.2

            gradient: Gradient {
                GradientStop { position: 0.0; color: Qt.rgba(255, 255, 255, 0.05) }
                GradientStop { position: 1.0; color: Qt.rgba(255, 255, 255, 0.02) }
            }

            opacity: parent.containsMouse ? 1 : 0
            Behavior on opacity {
                NumberAnimation { duration: 200 }
            }
        }

        // Main icon
        Kirigami.Icon {
            id: icon
            anchors.centerIn: parent
            width: Math.min(parent.width, parent.height) * 0.7
            height: width
            source: "audio-headphones"

            // Scale animation on hover
            scale: parent.containsMouse ? 1.1 : 1.0
            Behavior on scale {
                SpringAnimation {
                    spring: 5
                    damping: 0.5
                }
            }

            // Modern battery badge
            Item {
                visible: device && device.connected && averageBattery > 0
                anchors.bottom: parent.bottom
                anchors.right: parent.right
                anchors.margins: -2

                width: batteryBadge.width + Kirigami.Units.smallSpacing * 2
                height: batteryBadge.height + Kirigami.Units.smallSpacing

                // Shadow layers
                Rectangle {
                    anchors.fill: badgeBackground
                    anchors.topMargin: 1
                    anchors.leftMargin: 1
                    radius: badgeBackground.radius
                    color: Qt.rgba(0, 0, 0, 0.3)
                    z: -1
                }

                Rectangle {
                    id: badgeBackground
                    anchors.fill: parent
                    radius: height / 2

                    color: {
                        if (averageBattery < 20) return "#e74c3c"
                        if (averageBattery < 50) return "#f39c12"
                        return "#27ae60"
                    }

                    // Glass-like effect
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: 1
                        radius: parent.radius
                        color: Qt.rgba(255, 255, 255, 0.2)
                        z: 1
                    }

                    PlasmaComponents3.Label {
                        id: batteryBadge
                        anchors.centerIn: parent
                        text: averageBattery + "%"
                        font.pixelSize: Kirigami.Units.gridUnit * 0.5
                        font.bold: true
                        color: "white"
                        z: 2
                    }
                }
            }

            // Connection pulse
            Rectangle {
                visible: device && device.connected
                anchors.centerIn: parent
                width: parent.width * 1.3
                height: width
                radius: width / 2
                color: "transparent"
                border.width: 2
                border.color: "#27ae60"
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
                    NumberAnimation { from: 0.8; to: 1.2; duration: 3000 }
                }
            }
        }
    }
}
