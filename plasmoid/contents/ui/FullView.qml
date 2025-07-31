import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents3
import org.kde.plasma.extras as PlasmaExtras

Item {
    id: root

    property var devices: ({})
    property string selectedDevice: ""
    property var currentDevice: null

    signal deviceSelected(address: string)
    signal noiseControlChanged(mode: string)
    signal featureToggled(feature: string, enabled: bool)
    signal refreshRequested()

    width: Kirigami.Units.gridUnit * 24
    height: Kirigami.Units.gridUnit * 32

    // Gradient background with multiple layers for depth
    Rectangle {
        anchors.fill: parent
        color: Kirigami.ColorUtils.adjustColor(
            Kirigami.Theme.backgroundColor,
            {"value": -10}
        )
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: Qt.rgba(0.1, 0.1, 0.15, 0.3) }
            GradientStop { position: 1.0; color: Qt.rgba(0.05, 0.05, 0.1, 0.5) }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        spacing: Kirigami.Units.largeSpacing

        // Header
        RowLayout {
            Layout.fillWidth: true

            PlasmaExtras.Heading {
                Layout.fillWidth: true
                text: "kAirPods"
                level: 2
                color: Kirigami.Theme.textColor
                font.weight: Font.Light
            }
        }

        // Device selector card - only show if multiple devices
        Card {
            Layout.fillWidth: true
            title: i18n("Device")
            showTitle: false
            implicitHeight: Object.keys(devices).length != 1 ? Kirigami.Units.gridUnit * 2.5 : Kirigami.Units.gridUnit * 2
            visible: Object.keys(devices).length > 0

            contentItem: Component {
                Loader {
                    Layout.fillWidth: true

                    sourceComponent: Object.keys(devices).length != 1 ? comboBoxComponent : labelComponent

                    Component {
                        id: labelComponent
                        RowLayout {
                            spacing: Kirigami.Units.smallSpacing

                            Kirigami.Icon {
                                source: "audio-headphones"
                                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                                Layout.preferredHeight: Kirigami.Units.iconSizes.small
                            }

                            PlasmaComponents3.Label {
                                Layout.fillWidth: true
                                text: currentDevice ? currentDevice.name : ""
                                font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.1
                            }
                        }
                    }

                    Component {
                        id: comboBoxComponent
                        PlasmaComponents3.ComboBox {
                            Layout.fillWidth: true
                            enabled: Object.keys(devices).length > 0

                            model: {
                                var items = []
                                var nameCount = {}

                                // Count occurrences of each device name
                                for (var addr in devices) {
                                    var device = devices[addr]
                                    if (nameCount[device.name]) {
                                        nameCount[device.name]++
                                    } else {
                                        nameCount[device.name] = 1
                                    }
                                }

                                // Populate items with device names, showing MAC if duplicates exist
                                for (var addr in devices) {
                                    var device = devices[addr]
                                    var displayName = device.name
                                    if (nameCount[device.name] > 1) {
                                        displayName += " (" + addr + ")"
                                    }
                                    items.push({
                                        text: displayName,
                                        value: addr
                                    })
                                }

                                if (items.length == 0) {
                                    items.push({ text: i18n("No devices"), value: "" })
                                }

                                return items
                            }
                            textRole: "text"
                            valueRole: "value"

                            currentIndex: {
                                var items = model
                                for (var i = 0; i < items.length; i++) {
                                    if (items[i].value === selectedDevice) {
                                        return i
                                    }
                                }
                                return 0
                            }

                            onCurrentValueChanged: {
                                if (currentValue !== selectedDevice) {
                                    deviceSelected(currentValue)
                                }
                            }
                        }
                    }
                }
            }
        }

        // Device info with fade animation
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            opacity: currentDevice && currentDevice.connected ? 1 : 0
            Behavior on opacity {
                NumberAnimation { duration: 300 }
            }

            ColumnLayout {
                anchors.fill: parent
                anchors.bottomMargin: Kirigami.Units.largeSpacing * 2
                spacing: Kirigami.Units.largeSpacing
                visible: parent.opacity > 0

                // Battery status
                BatteryStatus {
                    Layout.fillWidth: true
                    device: currentDevice
                }

                // Noise control
                NoiseControlPanel {
                    Layout.fillWidth: true
                    currentMode: currentDevice && currentDevice.noise_mode ? currentDevice.noise_mode : "off"
                    onModeChanged: function(mode) {
                        noiseControlChanged(mode)
                    }
                }
            }
        }

        // Disconnected state
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !currentDevice || !currentDevice.connected

            Column {
                anchors.centerIn: parent
                spacing: Kirigami.Units.largeSpacing

                Kirigami.Icon {
                    anchors.horizontalCenter: parent.horizontalCenter
                    source: "network-disconnect"
                    width: Kirigami.Units.gridUnit * 4
                    height: width
                    opacity: 0.5
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: i18n("No device connected")
                    font.pixelSize: Kirigami.Units.gridUnit * 0.8
                    color: Kirigami.Theme.textColor
                    opacity: 0.5
                }
            }
        }
    }
}
