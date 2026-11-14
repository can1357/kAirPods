import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.FormLayout {
    property alias cfg_autoPlayPause: autoPlayPauseCheckbox.checked
    property alias cfg_showPulseAnimation: pulseAnimationCheckbox.checked
    property alias cfg_showCompactPercentage: compactPercentageCheckbox.checked

    QQC2.CheckBox {
        id: autoPlayPauseCheckbox
        Kirigami.FormData.label: i18n("Media Control:")
        text: i18n("Auto pause/resume when AirPods are removed/inserted")
    }

    QQC2.CheckBox {
        id: compactPercentageCheckbox
        Kirigami.FormData.label: i18n("Panel:")
        text: i18n("Show battery percentage in the panel")
    }

    QQC2.CheckBox {
        id: pulseAnimationCheckbox
        Kirigami.FormData.label: i18n("Appearance:")
        text: i18n("Show pulse animation when connected")
    }
}
