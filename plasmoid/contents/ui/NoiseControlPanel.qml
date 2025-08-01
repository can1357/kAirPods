import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Card {
    id: root

    property string currentMode: "off"
    signal modeChanged(mode: string)

    title: i18n("Noise Cancellation")
    implicitHeight: Kirigami.Units.gridUnit * 12

    contentItem: Component {
        GridLayout {
            columns: 2
            rowSpacing: Kirigami.Units.largeSpacing
            columnSpacing: Kirigami.Units.largeSpacing

            // Off button
            NoiseControlButton {
                Layout.fillWidth: true
                Layout.fillHeight: true
                text: i18n("Off")
                icon: "audio-volume-muted"
                mode: "off"
                checked: currentMode === "off"
                onClicked: root.modeChanged("off")
            }

            // Noise Cancellation button
            NoiseControlButton {
                Layout.fillWidth: true
                Layout.fillHeight: true
                text: i18n("Active")
                icon: "audio-headphones"
                mode: "anc"
                checked: currentMode === "anc"
                onClicked: root.modeChanged("anc")
            }

            // Transparency button
            NoiseControlButton {
                Layout.fillWidth: true
                Layout.fillHeight: true
                text: i18n("Transparency")
                icon: "view-visible"
                mode: "transparency"
                checked: currentMode === "transparency"
                onClicked: root.modeChanged("transparency")
            }

            // Adaptive button
            NoiseControlButton {
                Layout.fillWidth: true
                Layout.fillHeight: true
                text: i18n("Adaptive")
                icon: "view-refresh"
                mode: "adaptive"
                checked: currentMode === "adaptive"
                onClicked: root.modeChanged("adaptive")
            }
        }
    }
}
