package net.velcore.hyperion.account

interface AccountRegistry {
    suspend fun getAll(): List<AccountId>
    suspend fun add(accountId: AccountId)
    suspend fun remove(accountId: AccountId)
}