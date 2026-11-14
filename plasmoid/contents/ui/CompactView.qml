import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents3

Item {
    id: root

    property var device: null
    property bool showPulseAnimation: true
    property bool showPercentage: false
    property bool horizontalPanel: false
    Layout.minimumWidth: showPercentage && horizontalPanel
                         ? percentageLabel.implicitWidth + Kirigami.Units.smallSpacing + Kirigami.Units.iconSizes.medium
                         : -1
    Layout.preferredWidth: Layout.minimumWidth
    signal clicked()

    property var batteryLevels: {
        if (!device || !device.connected || !device.battery) return []
        const battery = device.battery
        if (battery.headphone?.level != null) return [battery.headphone.level]
        const levels = []
        if (battery.left?.level != null) levels.push(battery.left.level)
        if (battery.right?.level != null) levels.push(battery.right.level)
        return levels
    }
    property int averageBattery: batteryLevels.length
                                 ? Math.round(batteryLevels.reduce((a, b) => a + b, 0) / batteryLevels.length)
                                 : 0
    property int lowestBattery: batteryLevels.length ? Math.min(...batteryLevels) : -1

    MouseArea {
        id: mouseArea
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

        Item {
            id: content
            anchors.centerIn: parent
            property real iconSize: Math.round(Math.min(root.height * 0.7, Kirigami.Units.iconSizes.medium))
            property bool showIcon: !root.showPercentage
                                    || root.width >= percentageLabel.implicitWidth + Kirigami.Units.smallSpacing + iconSize
            width: root.showPercentage
                   ? percentageLabel.width + (showIcon ? iconSize + Kirigami.Units.smallSpacing : 0)
                   : Math.round(Math.min(root.width, root.height) * 0.7)
            height: root.height

            // Main icon
            Kirigami.Icon {
                id: icon
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                visible: content.showIcon
                width: root.showPercentage ? content.iconSize : content.width
                height: width
                source: "audio-headphones"

                // Scale animation on hover
                scale: mouseArea.containsMouse ? 1.1 : 1.0
                Behavior on scale {
                    SpringAnimation {
                        spring: 5
                        damping: 0.5
                    }
                }

                // Modern battery badge
                Item {
                    visible: !root.showPercentage && batteryLevels.length > 0
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
                            text: i18nc("Battery level; %1 is a percentage", "%1%", averageBattery)
                            font.pixelSize: Kirigami.Units.gridUnit * 0.5
                            font.bold: true
                            color: "white"
                            z: 2
                        }
                    }
                }

                // Connection pulse
                Rectangle {
                    visible: showPulseAnimation && device && device.connected
                    anchors.centerIn: parent
                    width: parent.width + 2 * Math.round(parent.width * 0.15)
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

            PlasmaComponents3.Label {
                id: percentageLabel
                visible: root.showPercentage
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                anchors.leftMargin: content.showIcon ? content.iconSize + Kirigami.Units.smallSpacing : 0
                width: Math.min(implicitWidth, root.width)
                font.pixelSize: Kirigami.Units.gridUnit * 0.7
                fontSizeMode: Text.HorizontalFit
                minimumPixelSize: 6
                text: root.lowestBattery >= 0 ? i18nc("Battery level; %1 is a percentage", "%1%", root.lowestBattery) : "—"
            }
        }
    }
}
