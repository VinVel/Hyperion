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
