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
import net.velcore.hyperion.account.auth.LoginService
import net.velcore.hyperion.account.session.MatrixClientFactory
import net.velcore.hyperion.account.session.MatrixSession

class AccountManager(
    private val accountStore: AccountStore
) {

    private val sessions = mutableMapOf<String, MatrixSession>()

    suspend fun login(
        baseUrl: String,
        username: String,
        password: String
    ): MatrixSession {

        val login = LoginService().login(baseUrl, username, password)

        val session = MatrixSession(
            MatrixClientFactory.create(login)
        )

        accountStore.save(
            StoredAccount(
                userId = login.userId,
                baseUrl = baseUrl,
                accessToken = login.accessToken,
                deviceId = login.deviceId
            )
        )

        sessions[login.userId.toString()] = session

        return session
    }

    suspend fun restoreAll() {

        val stored = accountStore.loadAll()

        for (account in stored) {
            val client = MatrixClientFactory.restore(account)
            sessions[account.userId.toString()] = MatrixSession(MatrixClient)
        }
    }

    fun get(userId: String): MatrixSession? =
        sessions[userId]
}