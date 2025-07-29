import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Card {
    id: root

    property var battery: null
    property var earDetection: null

    title: "Battery"
    implicitHeight: Kirigami.Units.gridUnit * 8

    contentItem: Component {
        RowLayout {
            spacing: Kirigami.Units.largeSpacing

            // Left AirPod
            CircularBatteryIndicator {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignCenter
                visible: battery && battery.left_available
                label: "L"
                level: battery ? battery.left_level : 0
                charging: battery ? battery.left_charging : false
                size: Kirigami.Units.gridUnit * 3.5
                showEarStatus: true
                inEar: earDetection ? earDetection.left_in_ear : false
            }

            // Right AirPod
            CircularBatteryIndicator {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignCenter
                visible: battery && battery.right_available
                label: "R"
                level: battery ? battery.right_level : 0
                charging: battery ? battery.right_charging : false
                size: Kirigami.Units.gridUnit * 3.5
                showEarStatus: true
                inEar: earDetection ? earDetection.right_in_ear : false
            }

            // Case
            //CircularBatteryIndicator {
            //    Layout.fillWidth: true
            //    Layout.alignment: Qt.AlignCenter
            //    visible: battery && battery.case !== undefined && battery.case !== null
            //    label: "Case"
            //    level: battery ? battery.case : 0
            //    charging: battery ? battery.case_charging : false
            //    size: Kirigami.Units.gridUnit * 3.5
            //}
        }
    }
}
