package net.velcore.hyperion.account

import de.connect2x.trixnity.client.MatrixClient
import de.connect2x.trixnity.clientserverapi.client.MatrixClientAuthProviderData

interface MatrixClientCreator {
    suspend fun create(authProviderData: MatrixClientAuthProviderData): Result<MatrixClient>
    suspend fun restore(accountId: AccountId): Result<MatrixClient>
}