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

package net.velcore.hyperion.infrastructure.network

import de.connect2x.trixnity.client.CryptoDriverModule
import de.connect2x.trixnity.client.MatrixClient
import de.connect2x.trixnity.client.MediaStoreModule
import de.connect2x.trixnity.client.RepositoriesModule
import de.connect2x.trixnity.client.create
import de.connect2x.trixnity.clientserverapi.client.MatrixClientAuthProviderData
import io.ktor.client.engine.HttpClientEngine
import net.velcore.hyperion.account.AccountId
import net.velcore.hyperion.account.MatrixClientCreator

//Responsible for creating configured MatrixClient instances.
//Injects platform-specific HttpClientEngine and required Trixnity modules.
class MatrixClientFactory(
    private val clientEngine: HttpClientEngine,
    private val repositoriesModule: RepositoriesModule,
    private val mediaStoreModule: MediaStoreModule,
    private val cryptoDriverModule: CryptoDriverModule,
): MatrixClientCreator {
    //Creates a MatrixClient using the provided authentication data.
    override suspend fun create(authProviderData: MatrixClientAuthProviderData): Result<MatrixClient> {
        return MatrixClient.create(
            repositoriesModule = repositoriesModule,
            mediaStoreModule = mediaStoreModule,
            cryptoDriverModule = cryptoDriverModule,
            authProviderData = authProviderData
        ) {
            httpClientEngine = clientEngine
        }
    }

    override suspend fun restore(accountId: AccountId): Result<MatrixClient> {
        return MatrixClient.create(
            repositoriesModule = repositoriesModule,
            mediaStoreModule = mediaStoreModule,
            cryptoDriverModule = cryptoDriverModule,
            authProviderData = null
        ) {
            httpClientEngine = clientEngine
        }
    }
}