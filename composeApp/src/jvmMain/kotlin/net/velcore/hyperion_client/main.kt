package net.velcore.hyperion_client

import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application

fun main() = application {
    Window(
        onCloseRequest = ::exitApplication,
        title = "hyperion_client",
    ) {
        App()
    }
}