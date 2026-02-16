package net.velcore.hyperion_client

import net.velcore.hyperion_client.account.AccountManager

class AppStartup(private val accountManager: AccountManager) {
    suspend fun start() {
        accountManager.restore()
    }
}
