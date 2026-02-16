package net.velcore.hyperion_client.account

interface AccountRegistry {
    suspend fun getAll(): List<AccountId>
    suspend fun add(accountId: AccountId)
    suspend fun remove(accountId: AccountId)
}