package net.velcore.hyperion_client.infrastructure.network

import de.connect2x.trixnity.client.CryptoDriverModule
import de.connect2x.trixnity.client.MatrixClient
import de.connect2x.trixnity.client.MediaStoreModule
import de.connect2x.trixnity.client.RepositoriesModule
import de.connect2x.trixnity.client.create
import de.connect2x.trixnity.clientserverapi.client.MatrixClientAuthProviderData
import io.ktor.client.engine.HttpClientEngine

//Responsible for creating configured MatrixClient instances.
//Injects platform-specific HttpClientEngine and required Trixnity modules.
class MatrixClientFactory(
    private val engine: HttpClientEngine,
    private val repositoriesModule: RepositoriesModule,
    private val mediaStoreModule: MediaStoreModule,
    private val cryptoDriverModule: CryptoDriverModule,
) {
    //Creates a MatrixClient using the provided authentication data.
    suspend fun create(
        authProviderData: MatrixClientAuthProviderData
    ): Result<MatrixClient> {

        return MatrixClient.create(
            repositoriesModule = repositoriesModule,
            mediaStoreModule = mediaStoreModule,
            cryptoDriverModule = cryptoDriverModule,
            authProviderData = authProviderData
        ) {
            httpClientEngine = engine
        }
    }
}