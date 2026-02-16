package net.velcore.hyperion.infrastructure.network

import io.ktor.client.engine.HttpClientEngine

expect object HttpEngineProvider {
    fun provide(): HttpClientEngine
}