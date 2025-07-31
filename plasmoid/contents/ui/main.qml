import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.components as PlasmaComponents3
import org.kde.plasma.plasma5support as Plasma5Support
import org.kde.kirigami as Kirigami
import org.kde.plasma.workspace.dbus as DBus

PlasmoidItem {
    id: root

    Plasmoid.icon: "audio-headphones"

    preferredRepresentation: compactRepresentation
    switchWidth: Kirigami.Units.gridUnit * 12
    switchHeight: Kirigami.Units.gridUnit * 12

    // ------------------------------------------------------------------
    // Service availability watcher
    // ------------------------------------------------------------------
    DBus.DBusServiceWatcher {
        id: serviceWatcher
        busType: DBus.BusType.Session
        watchedService: "org.kairpods"

        onRegisteredChanged: {
            if (registered) {
                console.log("kAirPods service is available")
                managerProps.updateAll()
            } else {
                console.log("kAirPods service is not available")
                devices = {}
                selectedDevice = ""
                connectedCount = 0
            }
        }
    }

    // ------------------------------------------------------------------
    // D-Bus properties bridge (org.kairpods.manager)
    // ------------------------------------------------------------------
    DBus.Properties {
        id: managerProps
        busType: DBus.BusType.Session
        service: "org.kairpods"
        path: "/org/kairpods/manager"
        iface: "org.kairpods.manager"

        onRefreshed: syncFromProperties()

        onPropertiesChanged: function (ifaceName, changed, invalidated) {
            if (ifaceName !== "org.kairpods.manager")
                return

            if ((changed.Devices != null) ||
                invalidated.indexOf("Devices") !== -1) {
                syncFromProperties()
            }


            // ConnectedCount is a DBus.UINT32, extract its value
            if (changed.ConnectedCount?.value != null) {
                connectedCount = changed.ConnectedCount.value
            }
        }
    }

    // ------------------------------------------------------------------
    // Reactive state exposed to the UI
    // ------------------------------------------------------------------
    property var devices: ({})
    property string selectedDevice: ""
    property var currentDevice: devices[selectedDevice] || null
    property int connectedCount: 0

    function syncFromProperties() {
        if (!managerProps.properties) {
            console.log("Properties not ready yet")
            return
        }

        const props = managerProps.properties
        let raw = props.Devices
        let list = []
        try {
            list = JSON.parse(raw || "[]")
            console.log("Parsed devices:", list.length, "devices")
        } catch (e) {
            console.error("Failed to parse devices:", e)
            list = []
        }

        updateDevicesList(list)

        // ConnectedCount is a DBus.UINT32, so we need to access its value property
        if (props.ConnectedCount?.value != null) {
            connectedCount = props.ConnectedCount.value
        }
    }

    // ------------------------------------------------------------------
    // D-Bus helpers (fire-and-forget – updates come via PropertiesChanged)
    // ------------------------------------------------------------------
    function sendCommand(action, params) {
        if (!selectedDevice || !serviceWatcher.registered) return
        DBus.SessionBus.asyncCall({
            service: "org.kairpods",
            path: "/org/kairpods/manager",
            iface: "org.kairpods.manager",
            member: "SendCommand",
            arguments: [selectedDevice, action, params]
        })
    }

    function connectDevice(address) {
        if (!serviceWatcher.registered) return
        DBus.SessionBus.asyncCall({
            service: "org.kairpods",
            path: "/org/kairpods/manager",
            iface: "org.kairpods.manager",
            member: "ConnectDevice",
            arguments: [address]
        })
    }

    function disconnectDevice(address) {
        if (!serviceWatcher.registered) return
        DBus.SessionBus.asyncCall({
            service: "org.kairpods",
            path: "/org/kairpods/manager",
            iface: "org.kairpods.manager",
            member: "DisconnectDevice",
            arguments: [address]
        })
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------
    function updateDevicesList(deviceList) {
        var newDevices = {}
        for (var i = 0; i < deviceList.length; ++i) {
            var d = deviceList[i]
            if (d && d.address)
                newDevices[d.address] = d
        }
        devices = newDevices

        if (!selectedDevice && deviceList.length > 0)
            selectedDevice = deviceList[0].address

        if (selectedDevice && !devices.hasOwnProperty(selectedDevice))
            selectedDevice = ""
    }

    // ------------------------------------------------------------------
    // UI representations
    // ------------------------------------------------------------------
    compactRepresentation: CompactView {
        device: root.currentDevice
        onClicked: root.expanded = !root.expanded
    }

    fullRepresentation: Item {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        Layout.preferredHeight: Kirigami.Units.gridUnit * 20

        // Service unavailable view
        ColumnLayout {
            anchors.centerIn: parent
            visible: !serviceWatcher.registered
            spacing: Kirigami.Units.largeSpacing

            Kirigami.Icon {
                source: "face-sad"
                Layout.preferredWidth: Kirigami.Units.iconSizes.huge
                Layout.preferredHeight: Kirigami.Units.iconSizes.huge
                Layout.alignment: Qt.AlignHCenter
            }

            PlasmaComponents3.Label {
                text: i18n("kAirPods Service Unavailable")
                font.bold: true
                Layout.alignment: Qt.AlignHCenter
            }

            PlasmaComponents3.Label {
                text: i18n("The kAirPods service is not installed or not running")
                wrapMode: Text.WordWrap
                horizontalAlignment: Text.AlignHCenter
                Layout.fillWidth: true
                Layout.maximumWidth: Kirigami.Units.gridUnit * 15
            }

            PlasmaComponents3.Button {
                text: i18n("Install kAirPods")
                icon.name: "download"
                Layout.alignment: Qt.AlignHCenter
                onClicked: Qt.openUrlExternally("https://github.com/can1357/kAirPods")
            }
        }

        // Normal device view
        FullView {
            anchors.fill: parent
            visible: serviceWatcher.registered

            devices: root.devices
            selectedDevice: root.selectedDevice
            currentDevice: root.currentDevice

            onDeviceSelected: function (address) {
                root.selectedDevice = address
            }

            onNoiseControlChanged: function (mode) {
                root.sendCommand("set_noise_mode", { value: mode })
            }

            onFeatureToggled: function (feature, enabled) {
                root.sendCommand("set_feature", { feature: feature, enabled: enabled })
            }

            onRefreshRequested: managerProps.updateAll()
        }
    }

    Component.onCompleted: {
        if (serviceWatcher.registered) {
            managerProps.updateAll()
        }
    }
}
