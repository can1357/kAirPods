import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Card {
    id: root
    
    property string currentMode: "off"
    signal modeChanged(mode: string)
    
    title: "Noise Cancellation"
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
                text: "Off"
                icon: "audio-volume-muted"
                mode: "off"
                checked: currentMode === "off"
                onClicked: root.modeChanged("off")
            }
            
            // Noise Cancellation button
            NoiseControlButton {
                Layout.fillWidth: true
                Layout.fillHeight: true
                text: "Noise\nCancellation"
                icon: "audio-headphones"
                mode: "nc"
                checked: currentMode === "nc"
                onClicked: root.modeChanged("nc")
            }
            
            // Transparency button
            NoiseControlButton {
                Layout.fillWidth: true
                Layout.fillHeight: true
                text: "Transparency"
                icon: "view-visible"
                mode: "trans"
                checked: currentMode === "trans"
                onClicked: root.modeChanged("trans")
            }
            
            // Adaptive button
            NoiseControlButton {
                Layout.fillWidth: true
                Layout.fillHeight: true
                text: "Adaptive"
                icon: "im-status-away"
                mode: "adapt"
                checked: currentMode === "adapt"
                onClicked: root.modeChanged("adapt")
            }
        }
    }
}