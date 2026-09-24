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
    title: qsTr("Hello World")
    visible: true
    width: 640
    color: palette.window

    readonly property MyObject myObject: MyObject {
        number: 1
        string: qsTr("My String with my number: %1").arg(number)
    }

    Column {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 10

        Label {
            text: qsTr("Number: %1").arg(root.myObject.number)
            color: palette.text
        }

        Label {
            text: qsTr("String: %1").arg(root.myObject.string)
            color: palette.text
        }

        Button {
            text: qsTr("Increment Number")

            onClicked: root.myObject.incrementNumber()
        }

        Button {
            text: qsTr("Say Hi!")

            onClicked: root.myObject.sayHi(root.myObject.string, root.myObject.number)
        }

        Button {
            text: qsTr("Quit")

            onClicked: Qt.quit()
        }
    }
}
