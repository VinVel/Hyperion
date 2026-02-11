package net.velcore.hyperion_client

class JsPlatform : Platform {
    override val name: String = "Web with Kotlin/JS"
}

actual fun getPlatform(): Platform = JsPlatform()