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

package net.velcore.hyperion.infrastructure.utils

import de.connect2x.trixnity.clientserverapi.client.MatrixClientServerApiClient
import de.connect2x.trixnity.clientserverapi.client.UIA
import de.connect2x.trixnity.clientserverapi.model.authentication.AccountType
import de.connect2x.trixnity.clientserverapi.model.authentication.Register
import de.connect2x.trixnity.clientserverapi.model.uia.AuthenticationRequest

suspend fun MatrixClientServerApiClient.register(
    username: String? = null,
    password: String,
): Result<UIA<Register.Response>> {
    return runCatching {
        val registerStep = authentication.register(
            password = password,
            username = username,
            accountType = AccountType.USER
        ).getOrThrow()

        if (registerStep is UIA.Success) return@runCatching registerStep

        require(registerStep is UIA.Step<Register.Response>)
        val registerResult = registerStep.authenticate(AuthenticationRequest.Dummy).getOrThrow()

        require(registerResult is UIA.Success<Register.Response>)
        registerResult
    }
}
