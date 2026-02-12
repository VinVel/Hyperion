package net.velcore.hyperion_client.sync

sealed class SyncState {
    object Idle
    object Syncing
    object Offline
    data class Error(val throwable: Throwable)
}