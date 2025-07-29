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

    // Device state
    property var devices: ({})
    property string selectedDevice: ""
    property var currentDevice: devices[selectedDevice] || null

    // Polling timer for updates
    Timer {
        id: pollTimer
        interval: 500
        repeat: true
        running: true
        onTriggered: {
            getDevices()
            if (selectedDevice) {
                getDeviceStatus()
            }
        }
    }

    // For now, we'll rely on polling via the Timer
    // D-Bus signal watching might need a different approach in Plasma 6

    // Direct D-Bus calls
    function getDevices() {

        var reply = DBus.SessionBus.asyncCall({
            service: "org.kde.plasma.airpods",
            path: "/org/kde/plasma/airpods",
            iface: "org.kde.plasma.airpods",
            member: "GetDevices",
            arguments: []
        })

        reply.finished.connect(function() {

            if (reply.isValid && !reply.isError) {
                try {
                    var deviceList = JSON.parse(reply.value);
                    updateDevicesList(deviceList)
                } catch (e) {
                    updateDevicesList([])
                }
            } else {
            }
        })
    }

    function getDeviceStatus() {
        if (!selectedDevice) return


        var reply = DBus.SessionBus.asyncCall({
            service: "org.kde.plasma.airpods",
            path: "/org/kde/plasma/airpods",
            iface: "org.kde.plasma.airpods",
            member: "GetDevice",
            arguments: [selectedDevice]
        })

        reply.finished.connect(function() {
            if (reply.isValid && !reply.isError) {
                try {
                    var jsonString = reply.value
                    var device = JSON.parse(jsonString)
                    if (device && device.address) {
                        var temp = JSON.parse(JSON.stringify(devices))
                        temp[device.address] = device
                        devices = temp
                    }
                } catch (e) {
                }
            } else {
            }
        })
    }

    function sendCommand(action, params) {
        if (!selectedDevice) return

        var reply = DBus.SessionBus.asyncCall({
            service: "org.kde.plasma.airpods",
            path: "/org/kde/plasma/airpods",
            iface: "org.kde.plasma.airpods",
            member: "SendCommand",
            arguments: [selectedDevice, action, params]
        })

        reply.finished.connect(function() {
            if (reply.isValid && !reply.isError) {
                getDeviceStatus() // Refresh
            } else {
            }
        })
    }

    // Connect device
    function connectDevice(address) {
        var reply = DBus.SessionBus.asyncCall({
            service: "org.kde.plasma.airpods",
            path: "/org/kde/plasma/airpods",
            iface: "org.kde.plasma.airpods",
            member: "ConnectDevice",
            arguments: [address]
        })

        reply.finished.connect(function() {
            if (reply.isValid && !reply.isError) {
                getDevices()
            } else {
            }
        })
    }

    // Disconnect device
    function disconnectDevice(address) {
        var reply = DBus.SessionBus.asyncCall({
            service: "org.kde.plasma.airpods",
            path: "/org/kde/plasma/airpods",
            iface: "org.kde.plasma.airpods",
            member: "DisconnectDevice",
            arguments: [address]
        })

        reply.finished.connect(function() {
            if (reply.isValid && !reply.isError) {
                getDevices()
            } else {
            }
        })
    }

    // Update devices list
    function updateDevicesList(deviceList) {
        var newDevices = {}
        for (var i = 0; i < deviceList.length; i++) {
            var device = deviceList[i]
            if (device && device.address) {
                newDevices[device.address] = device
            }
        }
        devices = newDevices

        // Auto-select first device if none selected
        if (!selectedDevice && deviceList.length > 0) {
            selectedDevice = deviceList[0].address
        }
    }


    // Compact representation
    compactRepresentation: CompactView {
        device: root.currentDevice
        onClicked: root.expanded = !root.expanded
    }

    // Full representation
    fullRepresentation: FullView {
        devices: root.devices
        selectedDevice: root.selectedDevice
        currentDevice: root.currentDevice

        onDeviceSelected: function(address) {
            root.selectedDevice = address
            root.getDeviceStatus()
        }

        onNoiseControlChanged: function(mode) {
            root.sendCommand("set_noise_mode", { value: mode })
        }

        onFeatureToggled: function(feature, enabled) {
            root.sendCommand("set_feature", { feature: feature, enabled: enabled })
        }

        onRefreshRequested: {
            root.getDevices()
        }
    }

    // Initial fetch timer
    Timer {
        id: initialFetchTimer
        interval: 500
        repeat: false
        running: false
        onTriggered: {
            getDevices()
        }
    }

    Component.onCompleted: {
        initialFetchTimer.start()
    }
}
