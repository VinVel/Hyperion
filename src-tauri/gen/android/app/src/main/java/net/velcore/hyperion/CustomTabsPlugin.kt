package net.velcore.hyperion

import android.app.Activity
import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class OpenCustomTabArgs {
  lateinit var url: String
}

@TauriPlugin
class CustomTabsPlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun openUrl(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(OpenCustomTabArgs::class.java)
      val customTabsIntent =
        CustomTabsIntent.Builder()
          .setShowTitle(true)
          .setShareState(CustomTabsIntent.SHARE_STATE_OFF)
          .build()

      customTabsIntent.launchUrl(activity, Uri.parse(args.url))
      invoke.resolve()
    } catch (error: Exception) {
      invoke.reject("Failed to open Android Custom Tab: ${error.message}")
    }
  }
}
