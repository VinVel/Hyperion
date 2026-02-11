package net.velcore.hyperion_client

interface Platform {
    val name: String
}

expect fun getPlatform(): Platform