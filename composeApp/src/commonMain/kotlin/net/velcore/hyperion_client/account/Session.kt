package net.velcore.hyperion_client.account

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