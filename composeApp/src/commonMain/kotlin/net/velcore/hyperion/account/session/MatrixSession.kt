/*
 * Copyright (c) 2026 VinVel
 * SPDX-License-Identifier: AGPL-3.0-or-later
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, version 3.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 *
 * Project home: hyperion.velcore.net
 */

package net.velcore.hyperion.account.session

import de.connect2x.trixnity.client.MatrixClient

class MatrixSession(
    val client: MatrixClient
) {

    suspend fun start() {
        client.startSync()
    }

    suspend fun logout() {
        client.logout()
        client.closeSuspending()
    }
}