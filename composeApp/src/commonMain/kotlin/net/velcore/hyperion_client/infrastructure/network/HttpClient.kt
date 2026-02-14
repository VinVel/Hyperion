package net.velcore.hyperion_client.infrastructure.network

import io.ktor.client.engine.HttpClientEngine

expect object HttpEngineProvider {
    fun provide(): HttpClientEngine
}