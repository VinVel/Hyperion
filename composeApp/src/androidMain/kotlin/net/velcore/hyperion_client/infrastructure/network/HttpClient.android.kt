package net.velcore.hyperion_client.infrastructure.network

import io.ktor.client.engine.HttpClientEngine
import io.ktor.client.engine.okhttp.OkHttp

actual object HttpEngineProvider {
    actual fun provide(): HttpClientEngine = OkHttp.create()
}