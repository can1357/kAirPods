import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Card {
    id: root

    property string currentMode: "off"
    property bool adaptiveSupported: true
    signal modeChanged(mode: string)

    title: i18n("Noise Cancellation")

    contentItem: Component {
        GridLayout {
            columns: root.adaptiveSupported ? 2 : 3
            rowSpacing: Kirigami.Units.largeSpacing
            columnSpacing: Kirigami.Units.largeSpacing

            // Off button
            NoiseControlButton {
                Layout.fillWidth: true
                Layout.fillHeight: true
                text: i18nc("Noise control mode", "Off")
                icon: "audio-volume-muted"
                mode: "off"
                checked: currentMode === "off"
                onClicked: root.modeChanged("off")
            }

            // Noise Cancellation button
            NoiseControlButton {
                Layout.fillWidth: true
                Layout.fillHeight: true
                text: i18nc("Noise control mode: active noise cancellation", "Active")
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
                visible: root.adaptiveSupported
                checked: currentMode === "adaptive"
                onClicked: root.modeChanged("adaptive")
            }
        }
    }
}
