package net.velcore.hyperion

import android.annotation.SuppressLint
import android.app.Activity
import android.graphics.Color
import android.net.Uri
import android.view.Gravity
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import android.widget.ImageButton
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.ViewCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

private const val DEFAULT_DESKTOP_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 " +
    "(KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36 Hyperion/0.1"

@InvokeArg
class OpenOverlayWebViewArgs {
  lateinit var url: String
  var title: String? = null
  var userAgent: String? = null
}

@TauriPlugin
class MobileOverlayWebViewPlugin(private val activity: Activity) : Plugin(activity) {
  private var overlayContainer: FrameLayout? = null
  private var overlayWebView: WebView? = null

  @SuppressLint("SetJavaScriptEnabled")
  @Command
  fun open(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(OpenOverlayWebViewArgs::class.java)
      activity.runOnUiThread {
        closeOverlay()

        val root = activity.findViewById<ViewGroup>(android.R.id.content)
        val overlay =
          FrameLayout(activity).apply {
            layoutParams =
              FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
              )
            setBackgroundColor(Color.parseColor("#F2131724"))
            isClickable = true
            isFocusable = true
          }

        val sheet =
          LinearLayout(activity).apply {
            layoutParams =
              FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
              )
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#FF11141C"))
          }

        val toolbar =
          LinearLayout(activity).apply {
            layoutParams =
              LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
              )
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setBackgroundColor(Color.parseColor("#FF171B24"))
            minimumHeight = dpToPx(56)
            setPadding(dpToPx(16), dpToPx(8), dpToPx(8), dpToPx(8))
          }

        val titleView =
          TextView(activity).apply {
            layoutParams =
              LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
            text = args.title?.takeIf { it.isNotBlank() } ?: Uri.parse(args.url).host ?: args.url
            setTextColor(Color.WHITE)
            textSize = 16f
            maxLines = 1
          }

        val closeButton =
          ImageButton(activity).apply {
            layoutParams =
              LinearLayout.LayoutParams(dpToPx(40), dpToPx(40)).apply {
                marginStart = dpToPx(8)
              }
            contentDescription = "Close browser"
            setImageResource(android.R.drawable.ic_menu_close_clear_cancel)
            setBackgroundColor(Color.TRANSPARENT)
            imageTintList = android.content.res.ColorStateList.valueOf(Color.WHITE)
            setOnClickListener { closeOverlay() }
          }

        val webView =
          WebView(activity).apply {
            layoutParams =
              LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                0,
                1f,
              )
            settings.apply {
              javaScriptEnabled = true
              domStorageEnabled = true
              loadWithOverviewMode = true
              useWideViewPort = true
              builtInZoomControls = false
              displayZoomControls = false
              mixedContentMode = WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
              javaScriptCanOpenWindowsAutomatically = true
              setSupportMultipleWindows(false)
              cacheMode = WebSettings.LOAD_DEFAULT
              userAgentString = args.userAgent?.takeIf { it.isNotBlank() } ?: DEFAULT_DESKTOP_USER_AGENT
            }

            val cookieManager = CookieManager.getInstance()
            cookieManager.setAcceptCookie(true)
            cookieManager.setAcceptThirdPartyCookies(this, true)

            webChromeClient = WebChromeClient()
            webViewClient =
              object : WebViewClient() {
                override fun shouldOverrideUrlLoading(
                  view: WebView?,
                  request: WebResourceRequest?,
                ): Boolean = false
              }

            loadUrl(args.url)
          }

        toolbar.addView(titleView)
        toolbar.addView(closeButton)
        sheet.addView(toolbar)
        sheet.addView(webView)
        overlay.addView(sheet)

        ViewCompat.setOnApplyWindowInsetsListener(sheet) { _, insets ->
          val systemBars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
          toolbar.setPadding(dpToPx(16), systemBars.top + dpToPx(8), dpToPx(8), dpToPx(8))
          insets
        }

        root.addView(overlay)
        overlayContainer = overlay
        overlayWebView = webView
      }

      invoke.resolve()
    } catch (error: Exception) {
      invoke.reject("Failed to open mobile overlay webview: ${error.message}")
    }
  }

  private fun closeOverlay() {
    overlayWebView?.apply {
      stopLoading()
      webChromeClient = null
      loadUrl("about:blank")
      destroy()
    }
    overlayWebView = null

    overlayContainer?.let { container ->
      (container.parent as? ViewGroup)?.removeView(container)
    }
    overlayContainer = null
  }

  private fun dpToPx(dp: Int): Int =
    (dp * activity.resources.displayMetrics.density).toInt()
}
