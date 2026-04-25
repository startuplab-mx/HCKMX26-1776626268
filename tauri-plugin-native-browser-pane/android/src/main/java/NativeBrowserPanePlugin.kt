package com.hackathon404.nativebrowserpane

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.Outline
import android.util.TypedValue
import android.view.View
import android.view.ViewGroup
import android.view.ViewOutlineProvider
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class OpenArgs {
    lateinit var url: String
    var x: Double = 0.0
    var y: Double = 0.0
    var width: Double = 0.0
    var height: Double = 0.0
}

@InvokeArg
class BoundsArgs {
    var x: Double = 0.0
    var y: Double = 0.0
    var width: Double = 0.0
    var height: Double = 0.0
}

@InvokeArg
class NavigateArgs {
    lateinit var url: String
}

@TauriPlugin
class NativeBrowserPanePlugin(private val activity: Activity) : Plugin(activity) {
    private var webView: WebView? = null

    companion object {
        const val USER_AGENT =
            "Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"

        val BAD_PATTERNS = listOf(
            "porn", "xxx", "xvideos", "pornhub", "redtube", "youporn",
            "xnxx", "onlyfans", "chaturbate"
        )

        val FILTER_SCRIPT = """
        (function () {
          if (window.__sandboxFilterInstalled) return;
          window.__sandboxFilterInstalled = true;
          var BAD_TEXT = ["porn", "xxx", "nsfw"];
          function showBlocked() {
            try {
              document.open();
              document.write('<html><body style="font-family:sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#fde68a;color:#7c2d12"><div style="text-align:center;padding:32px"><div style="font-size:48px">🚫</div><h1>Sitio bloqueado</h1><p>Este contenido no está permitido.</p></div></body></html>');
              document.close();
            } catch (_) {}
          }
          function checkText() {
            var b = document.body;
            if (!b) return true;
            var t = (b.innerText || "").toLowerCase();
            if (t.length < 50) return true;
            for (var i = 0; i < BAD_TEXT.length; i++) {
              var occ = t.split(BAD_TEXT[i]).length - 1;
              if (occ >= 3) { showBlocked(); return false; }
            }
            return true;
          }
          function onReady() {
            if (!checkText()) return;
            try {
              var obs = new MutationObserver(function () { if (!checkText()) obs.disconnect(); });
              obs.observe(document.body, { childList: true, subtree: true, characterData: true });
            } catch (_) {}
          }
          if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", onReady);
          else onReady();
        })();
        """.trimIndent()
    }

    private val density: Float
        get() = activity.resources.displayMetrics.density

    private fun dpToPx(dp: Double): Int = (dp * density).toInt()

    private fun isUrlSafe(url: String): Boolean {
        val lower = url.lowercase()
        return BAD_PATTERNS.none { lower.contains(it) }
    }

    private fun emitEvent(name: String, url: String) {
        val data = JSObject()
        data.put("url", url)
        trigger(name, data)
    }

    private fun applyBounds(wv: WebView, x: Double, y: Double, width: Double, height: Double) {
        val params = FrameLayout.LayoutParams(dpToPx(width), dpToPx(height))
        params.leftMargin = dpToPx(x)
        params.topMargin = dpToPx(y)
        wv.layoutParams = params
        wv.invalidateOutline()
    }

    @Command
    fun open(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(OpenArgs::class.java)
            activity.runOnUiThread {
                if (!isUrlSafe(args.url)) {
                    emitEvent("browser-blocked", args.url)
                    invoke.reject("URL bloqueada")
                    return@runOnUiThread
                }

                // Si ya existe el webview, sólo reposiciona y navega.
                webView?.let { existing ->
                    applyBounds(existing, args.x, args.y, args.width, args.height)
                    existing.loadUrl(args.url)
                    invoke.resolve()
                    return@runOnUiThread
                }

                val wv = WebView(activity)
                wv.settings.javaScriptEnabled = true
                wv.settings.domStorageEnabled = true
                wv.settings.databaseEnabled = true
                wv.settings.userAgentString = USER_AGENT
                wv.setBackgroundColor(Color.WHITE)

                // Esquinas redondeadas (12dp) para matchear el placeholder paneRef.
                val cornerRadius = TypedValue.applyDimension(
                    TypedValue.COMPLEX_UNIT_DIP, 12f, activity.resources.displayMetrics
                )
                wv.outlineProvider = object : ViewOutlineProvider() {
                    override fun getOutline(view: View, outline: Outline) {
                        outline.setRoundRect(0, 0, view.width, view.height, cornerRadius)
                    }
                }
                wv.clipToOutline = true

                wv.webViewClient = object : WebViewClient() {
                    override fun shouldOverrideUrlLoading(
                        view: WebView,
                        request: WebResourceRequest
                    ): Boolean {
                        val url = request.url.toString()
                        if (!isUrlSafe(url)) {
                            emitEvent("browser-blocked", url)
                            return true
                        }
                        emitEvent("browser-navigated", url)
                        return false
                    }

                    override fun onPageStarted(
                        view: WebView?,
                        url: String?,
                        favicon: Bitmap?
                    ) {
                        super.onPageStarted(view, url, favicon)
                        view?.evaluateJavascript(FILTER_SCRIPT, null)
                    }
                }

                val rootView =
                    activity.findViewById<ViewGroup>(android.R.id.content)
                rootView.addView(wv)
                applyBounds(wv, args.x, args.y, args.width, args.height)
                wv.loadUrl(args.url)
                webView = wv
                invoke.resolve()
            }
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "open failed")
        }
    }

    @Command
    fun setBounds(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(BoundsArgs::class.java)
            activity.runOnUiThread {
                webView?.let {
                    applyBounds(it, args.x, args.y, args.width, args.height)
                }
                invoke.resolve()
            }
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "setBounds failed")
        }
    }

    @Command
    fun navigate(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(NavigateArgs::class.java)
            activity.runOnUiThread {
                if (!isUrlSafe(args.url)) {
                    emitEvent("browser-blocked", args.url)
                    invoke.reject("URL bloqueada")
                    return@runOnUiThread
                }
                webView?.loadUrl(args.url)
                invoke.resolve()
            }
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "navigate failed")
        }
    }

    @Command
    fun close(invoke: Invoke) {
        activity.runOnUiThread {
            webView?.let {
                it.stopLoading()
                it.webViewClient = WebViewClient()
                (it.parent as? ViewGroup)?.removeView(it)
                it.destroy()
            }
            webView = null
            invoke.resolve()
        }
    }
}
