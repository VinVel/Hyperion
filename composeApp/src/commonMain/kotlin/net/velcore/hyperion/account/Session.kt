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

package net.velcore.hyperion.account

import de.connect2x.trixnity.client.MatrixClient
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.job

class Session (val client: MatrixClient, parentScope: CoroutineScope) {
    private val job = SupervisorJob(parentScope.coroutineContext.job)
    private val scope = CoroutineScope(parentScope.coroutineContext + job)

    val loginState = client.loginState
    val syncState = client.syncState

    suspend fun startSync() {
        client.startSync()
    }

    suspend fun stopSync() {
        client.stopSync()
    }

    suspend fun logout(): Result<Unit> {
        return client.logout()
    }

    suspend fun close () {
        client.closeSuspending()
        job.cancel()
    }
}