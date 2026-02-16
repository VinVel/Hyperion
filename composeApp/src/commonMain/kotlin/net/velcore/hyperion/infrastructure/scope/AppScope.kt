package net.velcore.hyperion.infrastructure.scope

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelAndJoin

class AppScope {
    private val supervisorJob = SupervisorJob()

    val scope: CoroutineScope = CoroutineScope(supervisorJob + Dispatchers.Default)

    suspend fun shutdown() {
        supervisorJob.cancelAndJoin()
    }
}