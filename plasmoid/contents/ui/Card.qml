import QtQuick
import QtQuick.Controls
import org.kde.kirigami as Kirigami

Item {
    id: root
    
    property alias contentItem: contentLoader.sourceComponent
    property alias title: titleLabel.text
    property bool showTitle: true
    property real cardRadius: Kirigami.Units.gridUnit * 0.6
    property color backgroundColor: Kirigami.ColorUtils.adjustColor(
        Kirigami.Theme.backgroundColor, 
        {"alpha": -180}
    )
    
    // Layered shadow effect
    Rectangle {
        anchors.fill: parent
        anchors.topMargin: 3
        anchors.leftMargin: 1
        radius: cardRadius
        color: Qt.rgba(0, 0, 0, 0.08)
        z: -3
    }
    
    Rectangle {
        anchors.fill: parent
        anchors.topMargin: 2
        anchors.leftMargin: 0
        radius: cardRadius
        color: Qt.rgba(0, 0, 0, 0.12)
        z: -2
    }
    
    Rectangle {
        anchors.fill: parent
        anchors.topMargin: 1
        radius: cardRadius
        color: Qt.rgba(0, 0, 0, 0.15)
        z: -1
    }
    
    // Main background
    Rectangle {
        id: background
        anchors.fill: parent
        radius: cardRadius
        color: backgroundColor
        
        // Subtle border
        border.width: 1
        border.color: Kirigami.ColorUtils.adjustColor(
            Kirigami.Theme.textColor, 
            {"alpha": -220}
        )
        
        // Inner highlight for depth
        Rectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: parent.radius - 1
            color: "transparent"
            border.width: 1
            border.color: Kirigami.ColorUtils.adjustColor(
                Kirigami.Theme.backgroundColor, 
                {"lightness": 30, "alpha": -150}
            )
        }
    }
    
    // Content
    Column {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        spacing: Kirigami.Units.smallSpacing
        
        // Title
        Text {
            id: titleLabel
            visible: showTitle && text !== ""
            width: parent.width
            font.pixelSize: Kirigami.Units.gridUnit * 0.7
            font.weight: Font.Medium
            color: Kirigami.Theme.textColor
            opacity: 0.7
            text: ""
        }
        
        // Content loader
        Loader {
            id: contentLoader
            width: parent.width
            height: parent.height - (titleLabel.visible ? titleLabel.height + parent.spacing : 0)
        }
    }
    
    // Hover effect
    MouseArea {
        id: hoverArea
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.NoButton
    }
    
    states: [
        State {
            name: "hovered"
            when: hoverArea.containsMouse
            PropertyChanges {
                target: background
                color: Kirigami.ColorUtils.adjustColor(
                    Kirigami.Theme.backgroundColor, 
                    {"alpha": -150}
                )
                scale: 1.02
            }
        }
    ]
    
    transitions: Transition {
        ColorAnimation { duration: 200 }
        NumberAnimation { properties: "scale"; duration: 200; easing.type: Easing.OutCubic }
    }
}