package net.velcore.hyperion

import net.velcore.hyperion.account.AccountManager

class AppStartup(private val accountManager: AccountManager) {
    suspend fun start() {
        accountManager.restore()
    }
}
