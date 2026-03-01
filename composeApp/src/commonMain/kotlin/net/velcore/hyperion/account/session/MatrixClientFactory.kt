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

import de.connect2x.trixnity.client.CryptoDriverModule
import de.connect2x.trixnity.client.MatrixClient
import de.connect2x.trixnity.client.MediaStoreModule
import de.connect2x.trixnity.client.RepositoriesModule
import de.connect2x.trixnity.client.create
import de.connect2x.trixnity.clientserverapi.model.authentication.Login
import de.connect2x.trixnity.core.model.UserId
import net.velcore.hyperion.account.StoredAccount
import net.velcore.hyperion.platform.createCryptoDriverModule
import net.velcore.hyperion.platform.createMediaStore
import net.velcore.hyperion.platform.createRepositoriesModule
import org.koin.core.module.Module

object MatrixClientFactory {

    suspend fun create(
        baseUrl: String,
        login: Login.Response
    ): MatrixClient {
        return createClient(
            baseUrl = baseUrl,
            userId = login.userId,
            accessToken = login.accessToken,
            deviceId = login.deviceId ?: throw IllegalArgumentException("deviceId is required")
        )
    }

    suspend fun restore(account: StoredAccount): MatrixClient {
        return createClient(
            baseUrl = account.baseUrl,
            userId = account.userId,
            accessToken = account.accessToken,
            deviceId = account.deviceId
        )
    }

    private suspend fun createClient(
        baseUrl: String,
        userId: UserId,
        accessToken: String,
        deviceId: String
    ): MatrixClient {
        val repositoriesModule = createRepositoriesModule()
        val mediaStore = createMediaStore(userId)
        val cryptoDriverModule: Module = createCryptoDriverModule()

        return MatrixClient.create(
            repositoriesModule = RepositoriesModule(repositoriesModule),
            mediaStoreModule = MediaStoreModule(mediaStore),
            cryptoDriverModule = CryptoDriverModule()
        ) {
            baseUrl(baseUrl)
            userId(userId)
            accessToken(accessToken)
            deviceId(deviceId)
        }.getOrThrow()
    }
}
