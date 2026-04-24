import QtQuick 2.9
import QtQuick.Controls 2.2
import QtQuick.Layouts 1.2

import StreamingPreferences 1.0
import SystemProperties 1.0

GroupBox {
    property var page
    property alias resolutionComboBox: resolutionComboBox
    property alias bitrateSlider: slider

    width: parent.width - (parent.leftPadding + parent.rightPadding)
    padding: 12
    title: "<font color=\"skyblue\">" + qsTr("Basic Settings") + "</font>"
    font.pointSize: 12

    Column {
        anchors.fill: parent
        spacing: 5

        Label {
            width: parent.width
            id: resFPStitle
            text: qsTr("Resolution and FPS")
            font.pointSize: 12
            wrapMode: Text.Wrap
        }

        Label {
            width: parent.width
            id: resFPSdesc
            text: qsTr("Setting values too high for your PC or network connection may cause lag, stuttering, or errors.")
            font.pointSize: 9
            wrapMode: Text.Wrap
        }

        Row {
            spacing: 5
            width: parent.width

            AutoResizingComboBox {
                property int lastIndexValue

                function addDetectedResolution(friendlyNamePrefix, rect) {
                    var indexToAdd = 0
                    for (var j = 0; j < resolutionComboBox.count; j++) {
                        var existing_width = parseInt(resolutionListModel.get(j).video_width)
                        var existing_height = parseInt(resolutionListModel.get(j).video_height)

                        if (rect.width === existing_width && rect.height === existing_height) {
                            indexToAdd = -1
                            break
                        }
                        else if (rect.width * rect.height > existing_width * existing_height) {
                            indexToAdd = j + 1
                        }
                    }

                    if (indexToAdd >= 0) {
                        resolutionListModel.insert(indexToAdd,
                                                   {
                                                       "text": friendlyNamePrefix + " (" + rect.width + "x" + rect.height + ")",
                                                       "video_width": "" + rect.width,
                                                       "video_height": "" + rect.height,
                                                       "is_custom": false
                                                   })
                    }
                }

                Component.onCompleted: {
                    SystemProperties.refreshDisplays()

                    var done = false
                    for (var displayIndex = 0; !done; displayIndex++) {
                        var screenRect = SystemProperties.getNativeResolution(displayIndex)
                        var safeAreaRect = SystemProperties.getSafeAreaResolution(displayIndex)

                        if (screenRect.width === 0) {
                            done = true
                            break
                        }

                        addDetectedResolution(qsTr("Native"), screenRect)
                        addDetectedResolution(qsTr("Native (Excluding Notch)"), safeAreaRect)
                    }

                    var max_pixels = SystemProperties.maximumResolution.width * SystemProperties.maximumResolution.height
                    if (max_pixels > 0) {
                        for (var j = 0; j < resolutionComboBox.count; j++) {
                            var existing_width = parseInt(resolutionListModel.get(j).video_width)
                            var existing_height = parseInt(resolutionListModel.get(j).video_height)

                            if (existing_width * existing_height > max_pixels) {
                                resolutionListModel.remove(j)
                                j--
                            }
                        }
                    }

                    var saved_width = StreamingPreferences.width
                    var saved_height = StreamingPreferences.height
                    var index_set = false
                    for (var i = 0; i < resolutionListModel.count; i++) {
                        var el_width = parseInt(resolutionListModel.get(i).video_width)
                        var el_height = parseInt(resolutionListModel.get(i).video_height)

                        if (saved_width === el_width && saved_height === el_height) {
                            currentIndex = i
                            index_set = true
                            break
                        }
                    }

                    if (!index_set) {
                        resolutionListModel.append({
                                                       "text": qsTr("Custom") + " (" + StreamingPreferences.width + "x" + StreamingPreferences.height + ")",
                                                       "video_width": "" + StreamingPreferences.width,
                                                       "video_height": "" + StreamingPreferences.height,
                                                       "is_custom": true
                                                   })
                        currentIndex = resolutionListModel.count - 1
                    }
                    else {
                        resolutionListModel.append({
                                                       "text": qsTr("Custom"),
                                                       "video_width": "",
                                                       "video_height": "",
                                                       "is_custom": true
                                                   })
                    }

                    recalculateWidth()

                    lastIndexValue = currentIndex
                }

                id: resolutionComboBox
                maximumWidth: parent.width / 2
                textRole: "text"
                model: ListModel {
                    id: resolutionListModel

                    ListElement {
                        text: qsTr("720p")
                        video_width: "1280"
                        video_height: "720"
                        is_custom: false
                    }
                    ListElement {
                        text: qsTr("1080p")
                        video_width: "1920"
                        video_height: "1080"
                        is_custom: false
                    }
                    ListElement {
                        text: qsTr("1440p")
                        video_width: "2560"
                        video_height: "1440"
                        is_custom: false
                    }
                    ListElement {
                        text: qsTr("4K")
                        video_width: "3840"
                        video_height: "2160"
                        is_custom: false
                    }
                }

                function updateBitrateForSelection() {
                    var selectedWidth = parseInt(resolutionListModel.get(currentIndex).video_width)
                    var selectedHeight = parseInt(resolutionListModel.get(currentIndex).video_height)

                    if (StreamingPreferences.width !== selectedWidth || StreamingPreferences.height !== selectedHeight) {
                        StreamingPreferences.width = selectedWidth
                        StreamingPreferences.height = selectedHeight

                        if (StreamingPreferences.autoAdjustBitrate) {
                            StreamingPreferences.bitrateKbps = StreamingPreferences.getDefaultBitrate(StreamingPreferences.width,
                                                                                                      StreamingPreferences.height,
                                                                                                      StreamingPreferences.fps,
                                                                                                      StreamingPreferences.enableYUV444)
                            slider.value = StreamingPreferences.bitrateKbps
                        }
                    }

                    lastIndexValue = currentIndex
                }

                onActivated: {
                    if (resolutionListModel.get(currentIndex).is_custom) {
                        customResolutionDialog.open()
                    }
                    else {
                        updateBitrateForSelection()
                    }
                }

                NavigableDialog {
                    id: customResolutionDialog
                    standardButtons: Dialog.Ok | Dialog.Cancel

                    onOpened: {
                        widthField.forceActiveFocus()

                        if (customResolutionDialog.standardButton) {
                            customResolutionDialog.standardButton(Dialog.Ok).enabled = customResolutionDialog.isInputValid()
                        }
                    }

                    onClosed: {
                        widthField.clear()
                        heightField.clear()
                    }

                    onRejected: {
                        resolutionComboBox.currentIndex = resolutionComboBox.lastIndexValue
                    }

                    function isInputValid() {
                        if ((!widthField.acceptableInput && widthField.text) ||
                                (!heightField.acceptableInput && heightField.text)) {
                            return false
                        }

                        if ((!widthField.text && !widthField.placeholderText) ||
                                (!heightField.text && !heightField.placeholderText)) {
                            return false
                        }

                        return true
                    }

                    onAccepted: {
                        if (!isInputValid()) {
                            reject()
                            return
                        }

                        var width = widthField.text ? widthField.text : widthField.placeholderText
                        var height = heightField.text ? heightField.text : heightField.placeholderText

                        for (var i = 0; i < resolutionListModel.count; i++) {
                            if (resolutionListModel.get(i).is_custom) {
                                resolutionListModel.setProperty(i, "video_width", width)
                                resolutionListModel.setProperty(i, "video_height", height)
                                resolutionListModel.setProperty(i, "text", "Custom (" + width + "x" + height + ")")

                                resolutionComboBox.currentIndex = i
                                resolutionComboBox.updateBitrateForSelection()
                                resolutionComboBox.recalculateWidth()
                                break
                            }
                        }
                    }

                    ColumnLayout {
                        Label {
                            text: qsTr("Custom resolutions are not officially supported by GeForce Experience, so it will not set your host display resolution. You will need to set it manually while in game.") + "\n\n" +
                                  qsTr("Resolutions that are not supported by your client or host PC may cause streaming errors.") + "\n"
                            wrapMode: Label.WordWrap
                            Layout.maximumWidth: 300
                        }

                        Label {
                            text: qsTr("Enter a custom resolution:")
                            font.bold: true
                        }

                        RowLayout {
                            TextField {
                                id: widthField
                                maximumLength: 5
                                inputMethodHints: Qt.ImhDigitsOnly
                                placeholderText: resolutionListModel.get(resolutionComboBox.currentIndex).video_width
                                validator: IntValidator { bottom: 256; top: 8192 }
                                focus: true

                                onTextChanged: {
                                    if (customResolutionDialog.standardButton) {
                                        customResolutionDialog.standardButton(Dialog.Ok).enabled = customResolutionDialog.isInputValid()
                                    }
                                }

                                Keys.onReturnPressed: {
                                    customResolutionDialog.accept()
                                }

                                Keys.onEnterPressed: {
                                    customResolutionDialog.accept()
                                }
                            }

                            Label {
                                text: "x"
                                font.bold: true
                            }

                            TextField {
                                id: heightField
                                maximumLength: 5
                                inputMethodHints: Qt.ImhDigitsOnly
                                placeholderText: resolutionListModel.get(resolutionComboBox.currentIndex).video_height
                                validator: IntValidator { bottom: 256; top: 8192 }

                                onTextChanged: {
                                    if (customResolutionDialog.standardButton) {
                                        customResolutionDialog.standardButton(Dialog.Ok).enabled = customResolutionDialog.isInputValid()
                                    }
                                }

                                Keys.onReturnPressed: {
                                    customResolutionDialog.accept()
                                }

                                Keys.onEnterPressed: {
                                    customResolutionDialog.accept()
                                }
                            }
                        }
                    }
                }
            }

            AutoResizingComboBox {
                property int lastIndexValue

                function updateBitrateForSelection() {
                    var selectedFps = parseInt(model.get(fpsComboBox.currentIndex).video_fps)
                    if (StreamingPreferences.fps !== selectedFps) {
                        StreamingPreferences.fps = selectedFps

                        if (StreamingPreferences.autoAdjustBitrate) {
                            StreamingPreferences.bitrateKbps = StreamingPreferences.getDefaultBitrate(StreamingPreferences.width,
                                                                                                      StreamingPreferences.height,
                                                                                                      StreamingPreferences.fps,
                                                                                                      StreamingPreferences.enableYUV444)
                            slider.value = StreamingPreferences.bitrateKbps
                        }
                    }

                    lastIndexValue = currentIndex
                }

                NavigableDialog {
                    id: customFpsDialog
                    standardButtons: Dialog.Ok | Dialog.Cancel

                    function isInputValid() {
                        if (!fpsField.acceptableInput && fpsField.text) {
                            return false
                        }

                        if (!fpsField.text && !fpsField.placeholderText) {
                            return false
                        }

                        return true
                    }

                    onOpened: {
                        fpsField.forceActiveFocus()

                        if (customFpsDialog.standardButton) {
                            customFpsDialog.standardButton(Dialog.Ok).enabled = customFpsDialog.isInputValid()
                        }
                    }

                    onClosed: {
                        fpsField.clear()
                    }

                    onRejected: {
                        fpsComboBox.currentIndex = fpsComboBox.lastIndexValue
                    }

                    onAccepted: {
                        if (!isInputValid()) {
                            reject()
                            return
                        }

                        var fps = fpsField.text ? fpsField.text : fpsField.placeholderText

                        for (var i = 0; i < fpsListModel.count; i++) {
                            if (fpsListModel.get(i).is_custom) {
                                fpsListModel.setProperty(i, "video_fps", fps)
                                fpsListModel.setProperty(i, "text", qsTr("Custom (%1 FPS)").arg(fps))

                                fpsComboBox.currentIndex = i
                                fpsComboBox.updateBitrateForSelection()
                                fpsComboBox.recalculateWidth()
                                break
                            }
                        }
                    }

                    ColumnLayout {
                        Label {
                            text: qsTr("Enter a custom frame rate:")
                            font.bold: true
                        }

                        RowLayout {
                            TextField {
                                id: fpsField
                                maximumLength: 4
                                inputMethodHints: Qt.ImhDigitsOnly
                                placeholderText: fpsListModel.get(fpsComboBox.currentIndex).video_fps
                                validator: IntValidator { bottom: 10; top: 9999 }
                                focus: true

                                onTextChanged: {
                                    if (customFpsDialog.standardButton) {
                                        customFpsDialog.standardButton(Dialog.Ok).enabled = customFpsDialog.isInputValid()
                                    }
                                }

                                Keys.onReturnPressed: {
                                    customFpsDialog.accept()
                                }

                                Keys.onEnterPressed: {
                                    customFpsDialog.accept()
                                }
                            }
                        }
                    }
                }

                function addRefreshRateOrdered(fpsListModel, refreshRate, description, custom) {
                    var indexToAdd = 0
                    for (var j = 0; j < fpsListModel.count; j++) {
                        var existing_fps = parseInt(fpsListModel.get(j).video_fps)

                        if (refreshRate === existing_fps || (custom && fpsListModel.get(j).is_custom)) {
                            indexToAdd = -1
                            break
                        }
                        else if (refreshRate > existing_fps) {
                            indexToAdd = j + 1
                        }
                    }

                    if (indexToAdd >= 0) {
                        if (custom) {
                            indexToAdd = fpsListModel.count
                        }

                        fpsListModel.insert(indexToAdd,
                                            {
                                                "text": description,
                                                "video_fps": "" + refreshRate,
                                                "is_custom": custom
                                            })
                    }

                    return indexToAdd
                }

                function reinitialize() {
                    var done = false
                    for (var displayIndex = 0; !done; displayIndex++) {
                        var refreshRate = SystemProperties.getRefreshRate(displayIndex)
                        if (refreshRate === 0) {
                            done = true
                            break
                        }

                        addRefreshRateOrdered(fpsListModel, refreshRate, qsTr("%1 FPS").arg(refreshRate), false)
                    }

                    var saved_fps = StreamingPreferences.fps
                    var found = false
                    for (var i = 0; i < model.count; i++) {
                        var el_fps = parseInt(model.get(i).video_fps)

                        if (saved_fps === el_fps) {
                            currentIndex = i
                            found = true
                            break
                        }
                    }

                    if (!found) {
                        currentIndex = addRefreshRateOrdered(model, saved_fps, qsTr("Custom (%1 FPS)").arg(saved_fps), true)
                    }
                    else {
                        addRefreshRateOrdered(model, "", qsTr("Custom"), true)
                    }

                    recalculateWidth()

                    lastIndexValue = currentIndex
                }

                Component.onCompleted: {
                    reinitialize()
                    if (page) {
                        page.languageChanged.connect(reinitialize)
                    }
                }

                model: ListModel {
                    id: fpsListModel

                    ListElement {
                        text: qsTr("30 FPS")
                        video_fps: "30"
                        is_custom: false
                    }
                    ListElement {
                        text: qsTr("60 FPS")
                        video_fps: "60"
                        is_custom: false
                    }
                }

                id: fpsComboBox
                maximumWidth: parent.width / 2
                textRole: "text"
                onActivated: {
                    if (model.get(currentIndex).is_custom) {
                        customFpsDialog.open()
                    }
                    else {
                        updateBitrateForSelection()
                    }
                }
            }
        }

        Label {
            width: parent.width
            id: bitrateTitle
            text: qsTr("Video bitrate:")
            font.pointSize: 12
            wrapMode: Text.Wrap
        }

        Label {
            width: parent.width
            id: bitrateDesc
            text: qsTr("Lower the bitrate on slower connections. Raise the bitrate to increase image quality.")
            font.pointSize: 9
            wrapMode: Text.Wrap
        }

        Row {
            width: parent.width
            spacing: 5

            Slider {
                id: slider
                value: StreamingPreferences.bitrateKbps
                stepSize: 500
                from: 500
                to: StreamingPreferences.unlockBitrate ? 500000 : 150000
                snapMode: "SnapOnRelease"
                width: Math.min(bitrateDesc.implicitWidth, parent.width - (resetBitrateButton.visible ? resetBitrateButton.width + parent.spacing : 0))

                onValueChanged: {
                    bitrateTitle.text = qsTr("Video bitrate: %1 Mbps").arg(value / 1000.0)
                    StreamingPreferences.bitrateKbps = value
                }

                onMoved: {
                    StreamingPreferences.autoAdjustBitrate = false
                }

                Component.onCompleted: {
                    if (page) {
                        page.languageChanged.connect(valueChanged)
                    }
                }
            }

            Button {
                id: resetBitrateButton
                text: qsTr("Use Default (%1 Mbps)").arg(StreamingPreferences.getDefaultBitrate(StreamingPreferences.width, StreamingPreferences.height, StreamingPreferences.fps, StreamingPreferences.enableYUV444) / 1000.0)
                visible: StreamingPreferences.bitrateKbps !== StreamingPreferences.getDefaultBitrate(StreamingPreferences.width, StreamingPreferences.height, StreamingPreferences.fps, StreamingPreferences.enableYUV444)

                onClicked: {
                    var defaultBitrate = StreamingPreferences.getDefaultBitrate(StreamingPreferences.width, StreamingPreferences.height, StreamingPreferences.fps, StreamingPreferences.enableYUV444)
                    StreamingPreferences.bitrateKbps = defaultBitrate
                    StreamingPreferences.autoAdjustBitrate = true
                    slider.value = defaultBitrate
                }
            }
        }

        Label {
            width: parent.width
            id: windowModeTitle
            text: qsTr("Display mode")
            font.pointSize: 12
            wrapMode: Text.Wrap
            visible: SystemProperties.hasDesktopEnvironment
        }

        AutoResizingComboBox {
            id: windowModeComboBox
            visible: SystemProperties.hasDesktopEnvironment
            enabled: !SystemProperties.rendererAlwaysFullScreen
            hoverEnabled: true
            textRole: "text"

            function createModel() {
                var model = Qt.createQmlObject("import QtQuick 2.0; ListModel {}", parent, "")

                model.append({
                                 text: qsTr("Fullscreen"),
                                 val: StreamingPreferences.WM_FULLSCREEN
                             })

                model.append({
                                 text: qsTr("Borderless windowed"),
                                 val: StreamingPreferences.WM_FULLSCREEN_DESKTOP
                             })

                model.append({
                                 text: qsTr("Windowed"),
                                 val: StreamingPreferences.WM_WINDOWED
                             })

                for (var i = 0; i < model.count; i++) {
                    var thisWm = model.get(i).val
                    if (thisWm === StreamingPreferences.recommendedFullScreenMode) {
                        model.get(i).text += " " + qsTr("(Recommended)")
                        model.move(i, 0, 1)
                        break
                    }
                }

                return model
            }

            function reinitialize() {
                if (!visible) {
                    return
                }

                model = createModel()
                currentIndex = 0

                var savedWm = StreamingPreferences.windowMode
                for (var i = 0; i < model.count; i++) {
                    var thisWm = model.get(i).val
                    if (savedWm === thisWm) {
                        currentIndex = i
                        break
                    }
                }

                activated(currentIndex)
            }

            Component.onCompleted: {
                reinitialize()
                if (page) {
                    page.languageChanged.connect(reinitialize)
                }
            }

            onActivated: {
                StreamingPreferences.windowMode = model.get(currentIndex).val
            }

            ToolTip.delay: 1000
            ToolTip.timeout: 5000
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Fullscreen generally provides the best performance, but borderless windowed may work better with features like macOS Spaces, Alt+Tab, screenshot tools, on-screen overlays, etc.")
        }

        CheckBox {
            id: vsyncCheck
            width: parent.width
            hoverEnabled: true
            text: qsTr("V-Sync")
            font.pointSize: 12
            checked: StreamingPreferences.enableVsync

            onCheckedChanged: {
                StreamingPreferences.enableVsync = checked
            }

            ToolTip.delay: 1000
            ToolTip.timeout: 5000
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Disabling V-Sync allows sub-frame rendering latency, but it can display visible tearing")
        }

        CheckBox {
            id: framePacingCheck
            width: parent.width
            hoverEnabled: true
            text: qsTr("Frame pacing")
            font.pointSize: 12
            enabled: StreamingPreferences.enableVsync
            checked: StreamingPreferences.enableVsync && StreamingPreferences.framePacing

            onCheckedChanged: {
                StreamingPreferences.framePacing = checked
            }

            ToolTip.delay: 1000
            ToolTip.timeout: 5000
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Frame pacing reduces micro-stutter by delaying frames that come in too early")
        }
    }
}
