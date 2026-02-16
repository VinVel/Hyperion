package net.velcore.hyperion_client.account

interface ActiveAccountStore {
    suspend fun getLastActive(): AccountId?
    suspend fun setLastActive(accountId: AccountId?)
}