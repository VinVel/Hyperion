package net.velcore.hyperion_client.account

import de.connect2x.trixnity.clientserverapi.client.MatrixClientAuthProviderData
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import net.velcore.hyperion_client.infrastructure.scope.AppScope

class AccountManager(
    private val matrixClientCreator: MatrixClientCreator,
    private val accountRegistry: AccountRegistry,
    private val activeAccountStore: ActiveAccountStore,
    private val appScope: AppScope
) {
    private val sessions = mutableMapOf<AccountId, Session>()

    private val activeSessionMutable = MutableStateFlow<Session?>(null)
    val activeSession: StateFlow<Session?> = activeSessionMutable

    suspend fun restore() {

        val accounts = accountRegistry.getAll()

        for (accountId in accounts) {
            val result = matrixClientCreator.restore(accountId)
            if (result.isSuccess) {
                val client = result.getOrThrow()
                val session = Session(client, appScope.scope)
                sessions[accountId] = session
            }
        }

        val lastActive = activeAccountStore.getLastActive()

        if (lastActive != null && sessions.containsKey(lastActive)) {
            setActive(lastActive)
        }
    }

    suspend fun login(authData: MatrixClientAuthProviderData): Result<AccountId> {
        val result = matrixClientCreator.create(authData)

        if (result.isFailure) {
            return Result.failure(result.exceptionOrNull()!!)
        }

        val client = result.getOrThrow()

        val accountId = AccountId(client.userId.toString())

        val session = Session(
            client = client,
            parentScope = appScope.scope
        )

        sessions[accountId] = session
        accountRegistry.add(accountId)

        setActive(accountId)

        return Result.success(accountId)
    }

    suspend fun setActive(accountId: AccountId) {
        val newSession = sessions[accountId]?: error("Session not found for $accountId")

        val previousSession = activeSessionMutable.value

        if (previousSession == newSession) return

        previousSession?.stopSync()

        activeSessionMutable.value = newSession
        activeAccountStore.setLastActive(accountId)

        newSession.startSync()
    }

    suspend fun logout(accountId: AccountId) {
        val session = sessions.remove(accountId)?: return

        session.stopSync()
        session.logout()
        session.close()

        accountRegistry.remove(accountId)

        if (activeSessionMutable.value == session) {
            activeSessionMutable.value = null
            activeAccountStore.setLastActive(null)
        }
    }

    suspend fun shutdown() {
        sessions.values.forEach { session ->
            session.stopSync()
            session.close()
        }
        sessions.clear()
        activeSessionMutable.value = null
    }
}