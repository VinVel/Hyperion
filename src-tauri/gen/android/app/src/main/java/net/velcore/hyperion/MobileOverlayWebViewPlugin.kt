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
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

private const val DEFAULT_DESKTOP_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 " +
    "(KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36 Hyperion/0.1"
private const val DESKTOP_VIEWPORT_WIDTH = 1280
private const val DESKTOP_INITIAL_SCALE_PERCENT = 30
private const val DESKTOP_VIEW_SCRIPT_TEMPLATE =
  """
  (() => {
    const defineGetter = (target, property, value) => {
      try {
        Object.defineProperty(target, property, {
          configurable: true,
          get: () => value
        });
      } catch (_) {}
    };

    const createMediaQueryResult = (query, matches) => ({
      matches,
      media: query,
      onchange: null,
      addListener() {},
      removeListener() {},
      addEventListener() {},
      removeEventListener() {},
      dispatchEvent() { return false; }
    });

    const desktopUserAgent = "__HYPERION_DESKTOP_UA__";
    const desktopViewportWidth = __HYPERION_DESKTOP_VIEWPORT_WIDTH__;
    const viewportContent = "__HYPERION_VIEWPORT_CONTENT__";
    defineGetter(window.navigator, "userAgent", desktopUserAgent);
    defineGetter(window.navigator, "appVersion", desktopUserAgent);
    defineGetter(window.navigator, "platform", "Win32");
    defineGetter(window.navigator, "vendor", "Google Inc.");
    defineGetter(window.navigator, "maxTouchPoints", 0);

    if ("userAgentData" in window.navigator) {
      defineGetter(window.navigator, "userAgentData", {
        brands: [
          { brand: "Chromium", version: "134" },
          { brand: "Google Chrome", version: "134" }
        ],
        mobile: false,
        platform: "Windows",
        getHighEntropyValues: async () => ({
          architecture: "x86",
          bitness: "64",
          mobile: false,
          model: "",
          platform: "Windows",
          platformVersion: "10.0.0",
          uaFullVersion: "134.0.0.0"
        }),
        toJSON() {
          return {
            brands: this.brands,
            mobile: this.mobile,
            platform: this.platform
          };
        }
      });
    }

    const originalMatchMedia = window.matchMedia?.bind(window);
    if (originalMatchMedia) {
      window.matchMedia = (query) => {
        if (
          query.includes("pointer: coarse") ||
          query.includes("hover: none") ||
          query.includes("any-pointer: coarse")
        ) {
          return createMediaQueryResult(query, false);
        }
        if (
          query.includes("pointer: fine") ||
          query.includes("hover: hover") ||
          query.includes("any-pointer: fine")
        ) {
          return createMediaQueryResult(query, true);
        }
        return originalMatchMedia(query);
      };
    }

    const applyViewport = () => {
      let viewportMeta = document.querySelector('meta[name="viewport"]');
      if (!viewportMeta) {
        viewportMeta = document.createElement("meta");
        viewportMeta.setAttribute("name", "viewport");
        (document.head || document.documentElement).appendChild(viewportMeta);
      }
      viewportMeta.setAttribute("content", viewportContent);
    };

    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", applyViewport, { once: true });
    }
    applyViewport();
  })();
  """

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
        val resolvedUserAgent = args.userAgent?.takeIf { it.isNotBlank() } ?: DEFAULT_DESKTOP_USER_AGENT

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
              setSupportZoom(true)
              builtInZoomControls = true
              displayZoomControls = false
              mixedContentMode = WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
              javaScriptCanOpenWindowsAutomatically = true
              setSupportMultipleWindows(false)
              cacheMode = WebSettings.LOAD_DEFAULT
              userAgentString = resolvedUserAgent
            }

            if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
              WebViewCompat.addDocumentStartJavaScript(
                this,
                buildDesktopViewScript(resolvedUserAgent),
                setOf("*"),
              )
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

            setInitialScale(DESKTOP_INITIAL_SCALE_PERCENT)
            loadUrl(args.url, mapOf("User-Agent" to resolvedUserAgent))
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

  private fun buildDesktopViewScript(userAgent: String): String =
    DESKTOP_VIEW_SCRIPT_TEMPLATE
      .replace("__HYPERION_DESKTOP_UA__", escapeJavaScriptString(userAgent))
      .replace("__HYPERION_DESKTOP_VIEWPORT_WIDTH__", DESKTOP_VIEWPORT_WIDTH.toString())
      .replace(
        "__HYPERION_VIEWPORT_CONTENT__",
        "width=$DESKTOP_VIEWPORT_WIDTH, initial-scale=0.25",
      )

  private fun escapeJavaScriptString(value: String): String =
    value
      .replace("\\", "\\\\")
      .replace("\"", "\\\"")
      .replace("\n", "\\n")
      .replace("\r", "\\r")
}
