package net.velcore.hyperion.infrastructure.network

import io.ktor.client.engine.HttpClientEngine
import io.ktor.client.engine.js.Js

actual object HttpEngineProvider {
    actual fun provide(): HttpClientEngine = Js.create()
}