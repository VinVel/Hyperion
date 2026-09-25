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
import QtQuick.Dialogs
import QtCore

Item {
    id: root

    readonly property string documentsLocation: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)

    signal importFileSelected(url fileUrl)
    signal exportFileSelected(url fileUrl)
    signal importCanceled()
    signal exportCanceled()

    function openImport() {
        importDialog.open()
    }

    function openExport() {
        exportDialog.open()
    }

    FileDialog {
        id: importDialog

        fileMode: FileDialog.OpenFile
        nameFilters: [qsTr("Encrypted Matrix room keys (*.txt *.keys)")]

        onAccepted: root.importFileSelected(selectedFile)
        onRejected: root.importCanceled()
    }

    FileDialog {
        id: exportDialog

        fileMode: FileDialog.SaveFile
        nameFilters: [qsTr("Encrypted Matrix room keys (*.txt *.keys)")]
        defaultSuffix: "txt"
        selectedFile: root.documentsLocation.length > 0
                      ? root.documentsLocation + "/hyperion-room-keys.txt"
                      : ""

        onAccepted: root.exportFileSelected(selectedFile)
        onRejected: root.exportCanceled()
    }
}
