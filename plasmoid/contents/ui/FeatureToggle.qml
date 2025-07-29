import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Item {
    id: root
    
    property string text: ""
    property string feature: ""
    property bool checked: false
    signal toggled()
    
    implicitHeight: Kirigami.Units.gridUnit * 2.5
    
    Rectangle {
        anchors.fill: parent
        radius: Kirigami.Units.gridUnit * 0.4
        color: Kirigami.ColorUtils.adjustColor(
            Kirigami.Theme.backgroundColor,
            {"alpha": mouseArea.containsMouse ? -150 : -200}
        )
        
        Behavior on color {
            ColorAnimation { duration: 150 }
        }
        
        // Subtle border
        border.width: 1
        border.color: Kirigami.ColorUtils.adjustColor(
            Kirigami.Theme.textColor,
            {"alpha": -220}
        )
        
        scale: mouseArea.pressed ? 0.98 : 1.0
        Behavior on scale {
            NumberAnimation { duration: 100 }
        }
    }
    
    RowLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        spacing: Kirigami.Units.largeSpacing
        
        Text {
            Layout.fillWidth: true
            text: root.text
            font.pixelSize: Kirigami.Units.gridUnit * 0.7
            color: Kirigami.Theme.textColor
            elide: Text.ElideRight
        }
        
        Switch {
            checked: root.checked
            onToggled: root.toggled()
        }
    }
    
    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: true
        onClicked: {
            root.checked = !root.checked
            root.toggled()
        }
    }
}