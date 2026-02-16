package net.velcore.hyperion.account

interface ActiveAccountStore {
    suspend fun getLastActive(): AccountId?
    suspend fun setLastActive(accountId: AccountId?)
}