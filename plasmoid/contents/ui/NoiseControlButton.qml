import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Item {
    id: root
    
    property string text: ""
    property string icon: ""
    property string mode: ""
    property bool checked: false
    signal clicked()
    
    // Multi-layer shadow for elevation effect
    Rectangle {
        anchors.fill: card
        anchors.topMargin: checked ? 4 : 2
        anchors.leftMargin: 1
        radius: card.radius
        color: Qt.rgba(0, 0, 0, checked ? 0.15 : 0.08)
        z: -3
    }
    
    Rectangle {
        anchors.fill: card
        anchors.topMargin: checked ? 3 : 1
        radius: card.radius
        color: Qt.rgba(0, 0, 0, checked ? 0.2 : 0.12)
        z: -2
    }
    
    Rectangle {
        anchors.fill: card
        anchors.topMargin: checked ? 2 : 0
        radius: card.radius
        color: Qt.rgba(0, 0, 0, checked ? 0.25 : 0.15)
        z: -1
        
        Behavior on anchors.topMargin {
            NumberAnimation { duration: 200 }
        }
    }
    
    // Card background
    Rectangle {
        id: card
        anchors.fill: parent
        radius: Kirigami.Units.gridUnit * 0.5
        
        color: checked ? 
            Kirigami.ColorUtils.adjustColor(
                Kirigami.Theme.highlightColor,
                {"alpha": -200}
            ) : 
            Kirigami.ColorUtils.adjustColor(
                Kirigami.Theme.backgroundColor,
                {"alpha": -180}
            )
        
        border.width: checked ? 2 : 1
        border.color: checked ? 
            Kirigami.Theme.highlightColor : 
            Kirigami.ColorUtils.adjustColor(
                Kirigami.Theme.textColor,
                {"alpha": -200}
            )
        
        scale: mouseArea.pressed ? 0.95 : 1.0
        
        Behavior on scale {
            SpringAnimation {
                spring: 5
                damping: 0.5
            }
        }
        
        Behavior on color {
            ColorAnimation { duration: 200 }
        }
        
        // Blur-like effect with multiple semi-transparent layers
        Rectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: parent.radius - 1
            color: Kirigami.ColorUtils.adjustColor(
                Kirigami.Theme.backgroundColor,
                {"alpha": -240}
            )
            z: -1
        }
    }
    
    // Content
    ColumnLayout {
        anchors.centerIn: parent
        spacing: Kirigami.Units.smallSpacing
        
        // Icon with animation
        Kirigami.Icon {
            id: iconItem
            source: root.icon
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: Kirigami.Units.iconSizes.medium
            Layout.preferredHeight: Kirigami.Units.iconSizes.medium
            color: root.checked ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
            
            transform: Rotation {
                origin.x: iconItem.width / 2
                origin.y: iconItem.height / 2
                angle: root.checked ? 360 : 0
                
                Behavior on angle {
                    NumberAnimation {
                        duration: 500
                        easing.type: Easing.OutBack
                    }
                }
            }
        }
        
        // Label
        Text {
            text: root.text
            Layout.alignment: Qt.AlignHCenter
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: Kirigami.Units.gridUnit * 0.65
            font.weight: root.checked ? Font.DemiBold : Font.Normal
            color: root.checked ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
            
            Behavior on font.weight {
                PropertyAnimation { duration: 200 }
            }
        }
    }
    
    // Ripple effect on click
    Rectangle {
        id: ripple
        anchors.centerIn: parent
        width: 0
        height: width
        radius: width / 2
        color: Kirigami.Theme.highlightColor
        opacity: 0
        
        ParallelAnimation {
            id: rippleAnimation
            NumberAnimation {
                target: ripple
                property: "width"
                from: 0
                to: root.width * 1.5
                duration: 300
                easing.type: Easing.OutQuad
            }
            NumberAnimation {
                target: ripple
                property: "opacity"
                from: 0.3
                to: 0
                duration: 300
            }
        }
    }
    
    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: true
        onClicked: {
            rippleAnimation.start()
            root.clicked()
        }
    }
    
    // Hover glow
    Rectangle {
        anchors.fill: parent
        radius: card.radius
        color: "transparent"
        border.width: 2
        border.color: Kirigami.Theme.highlightColor
        opacity: mouseArea.containsMouse && !root.checked ? 0.3 : 0
        
        Behavior on opacity {
            NumberAnimation { duration: 200 }
        }
    }
}