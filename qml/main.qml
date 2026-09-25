/*
 * Copyright (c) 2026 VinVel
 *
 * SPDX-License-Identifier: AGPL-3.0-only
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, version 3 only.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 *
 * Project home: hyperion.velcore.net
 */

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtCore

// This must match the uri and version
// specified in the qml_module in the build.rs script.
import net.velcore.hyperion

ApplicationWindow {
    id: root
    height: 480
    title: qsTr("Hyperion")
    visible: true
    width: 640
    color: palette.window

    readonly property HyperionIpc ipc: HyperionIpc {}
    property int nextRequestId: 1
    property string pendingRoomKeyPassphrase: ""

    signal roomKeyCommandCompleted(int requestId, string resultJson)

    Component.onCompleted: ipc.initializeAppPaths(
                               StandardPaths.writableLocation(StandardPaths.AppDataLocation).toString(),
                               StandardPaths.writableLocation(StandardPaths.CacheLocation).toString())

    function chooseRoomKeyImport(passphrase) {
        if (Qt.platform.os === "ios") {
            const requestId = nextRequestId++
            const request = { passphrase: passphrase, file_url: "" }
            ipc.importRoomKeys(requestId, JSON.stringify(request))
            return
        }

        pendingRoomKeyPassphrase = passphrase
        roomKeyFilePicker.openImport()
    }

    function chooseRoomKeyExport(passphrase) {
        if (Qt.platform.os === "ios") {
            const requestId = nextRequestId++
            const request = { passphrase: passphrase, file_url: "" }
            ipc.exportRoomKeys(requestId, JSON.stringify(request))
            return
        }

        pendingRoomKeyPassphrase = passphrase
        roomKeyFilePicker.openExport()
    }

    RoomKeyFilePicker {
        id: roomKeyFilePicker

        onImportFileSelected: function(fileUrl) {
            const requestId = root.nextRequestId++
            const request = {
                passphrase: root.pendingRoomKeyPassphrase,
                file_url: fileUrl.toString()
            }
            root.ipc.importRoomKeys(requestId, JSON.stringify(request))
            root.pendingRoomKeyPassphrase = ""
        }

        onExportFileSelected: function(fileUrl) {
            const requestId = root.nextRequestId++
            const request = {
                passphrase: root.pendingRoomKeyPassphrase,
                file_url: fileUrl.toString()
            }
            root.ipc.exportRoomKeys(requestId, JSON.stringify(request))
            root.pendingRoomKeyPassphrase = ""
        }

        onImportCanceled: root.pendingRoomKeyPassphrase = ""
        onExportCanceled: root.pendingRoomKeyPassphrase = ""
    }

    Connections {
        target: root.ipc

        function onCommandCompleted(requestId, resultJson) {
            root.roomKeyCommandCompleted(requestId, resultJson)
        }
    }

    Column {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 10

        Button {
            text: qsTr("Quit")

            onClicked: Qt.quit()
        }
    }
}
