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

package net.velcore.hyperion.account.auth

import de.connect2x.trixnity.clientserverapi.client.MatrixClientServerApiClient
import de.connect2x.trixnity.clientserverapi.model.authentication.Login
import de.connect2x.trixnity.clientserverapi.model.authentication.LoginType

class LoginService {

    suspend fun login(
        baseUrl: String,
        username: String,
        password: String
    ): Login.Response {

        val api = MatrixClientServerApiClient(baseUrl = baseUrl)

        return api.authentication.login(
            type = LoginType.Password,
            identifier = Login.Identifier.User(username),
            password = password
        ).getOrThrow()
    }
}