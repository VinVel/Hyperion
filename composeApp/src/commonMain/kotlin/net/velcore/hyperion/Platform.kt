package net.velcore.hyperion

interface Platform {
    val name: String
}

expect fun getPlatform(): Platform