import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Card {
    id: root

    property var battery: null
    property var earDetection: null

    title: i18n("Battery")
    implicitHeight: Kirigami.Units.gridUnit * 8

    contentItem: Component {
        RowLayout {
            spacing: Kirigami.Units.largeSpacing

            // Left AirPod
            CircularBatteryIndicator {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignCenter
                visible: !!battery?.left_available
                label: i18n("L")
                level: battery?.left_level ?? 0
                charging: !!battery?.left_charging
                size: Kirigami.Units.gridUnit * 3.5
                showEarStatus: true
                inEar: !!earDetection?.left_in_ear
            }

            // Right AirPod
            CircularBatteryIndicator {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignCenter
                visible: !!battery?.right_available
                label: i18n("R")
                level: battery?.right_level ?? 0
                charging: !!battery?.right_charging
                size: Kirigami.Units.gridUnit * 3.5
                showEarStatus: true
                inEar: !!earDetection?.right_in_ear
            }
        }
    }
}
