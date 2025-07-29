import QtQuick
import QtQuick.Controls
import org.kde.kirigami as Kirigami

Item {
    id: root
    
    property bool checked: false
    property color checkedColor: Kirigami.Theme.highlightColor
    property color uncheckedColor: Qt.rgba(0.5, 0.5, 0.5, 0.3)
    
    signal toggled()
    
    implicitWidth: Kirigami.Units.gridUnit * 2.5
    implicitHeight: Kirigami.Units.gridUnit * 1.4
    
    // Track background
    Rectangle {
        id: track
        anchors.fill: parent
        radius: height / 2
        color: checked ? checkedColor : uncheckedColor
        
        Behavior on color {
            ColorAnimation { duration: 200 }
        }
        
        // Inner shadow for depth
        Rectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: parent.radius
            color: "transparent"
            border.width: 1
            border.color: Qt.rgba(0, 0, 0, 0.1)
        }
    }
    
    // Handle with manual shadow
    Item {
        id: handleContainer
        width: parent.height - 4
        height: width
        x: checked ? parent.width - width - 2 : 2
        anchors.verticalCenter: parent.verticalCenter
        
        Behavior on x {
            NumberAnimation {
                duration: 200
                easing.type: Easing.InOutQuad
            }
        }
        
        // Shadow layers
        Rectangle {
            anchors.centerIn: parent
            width: parent.width + 4
            height: parent.height + 4
            radius: width / 2
            color: Qt.rgba(0, 0, 0, 0.1)
            z: -2
        }
        
        Rectangle {
            anchors.centerIn: parent
            width: parent.width + 2
            height: parent.height + 2
            radius: width / 2
            color: Qt.rgba(0, 0, 0, 0.15)
            z: -1
        }
        
        // Handle
        Rectangle {
            id: handle
            anchors.fill: parent
            radius: width / 2
            color: "white"
            
            // Scale animation on press
            scale: mouseArea.pressed ? 0.9 : 1.0
            Behavior on scale {
                NumberAnimation { duration: 100 }
            }
        }
    }
    
    MouseArea {
        id: mouseArea
        anchors.fill: parent
        onClicked: {
            root.checked = !root.checked
            root.toggled()
        }
    }
}