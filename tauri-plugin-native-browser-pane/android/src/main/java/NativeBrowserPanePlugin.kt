package com.hackathon404.nativebrowserpane

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Outline
import android.graphics.Paint
import android.graphics.Rect
import android.util.Base64
import android.util.TypedValue
import android.view.View
import android.view.ViewGroup
import android.view.ViewOutlineProvider
import android.webkit.JavascriptInterface
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
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import kotlin.concurrent.thread
import kotlin.random.Random

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

        // FILTER_SCRIPT v2 — batched text vía FilterBridge.filterTexts +
        // IntersectionObserver para imágenes. Imagen sigue URL-based via
        // FilterBridge.filterImage (Android no tiene buen path para binario).
        val FILTER_SCRIPT = """
        (function () {
          if (window.__sandboxFilterInstalled) return;
          window.__sandboxFilterInstalled = true;
          try {
            var pre = document.createElement('style');
            pre.id = '__sandbox_pre';
            pre.textContent =
              "p:not(.__sb_done),h1:not(.__sb_done),h2:not(.__sb_done)," +
              "h3:not(.__sb_done),h4:not(.__sb_done),h5:not(.__sb_done)," +
              "h6:not(.__sb_done){color:transparent !important;text-shadow:none !important;}" +
              "img:not(.__sb_done){filter:blur(24px) !important;}";
            (document.head || document.documentElement).appendChild(pre);
          } catch (_) {}

          var BAD_URL_PATTERNS = [/porn/i,/xxx/i,/xvideos/i,/pornhub/i,/redtube/i,/youporn/i,/xnxx/i,/onlyfans/i,/chaturbate/i,/\bnsfw\b/i];
          var BAD_TEXT = ['porn','xxx','nsfw'];
          function showBlocked(reason) {
            try {
              document.open();
              document.write('<html><body style="font-family:sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#fde68a;color:#7c2d12"><div style="text-align:center;padding:32px"><div style="font-size:48px">🚫</div><h1>Sitio bloqueado</h1><p>'+(reason||'')+'</p></div></body></html>');
              document.close();
            } catch (_) {}
          }
          function checkUrl() {
            var u = location.href || '';
            for (var i = 0; i < BAD_URL_PATTERNS.length; i++) {
              if (BAD_URL_PATTERNS[i].test(u)) { showBlocked('URL bloqueada'); return false; }
            }
            return true;
          }
          function checkText() {
            var b = document.body;
            if (!b) return true;
            var t = (b.innerText || '').toLowerCase();
            if (t.length < 50) return true;
            for (var i = 0; i < BAD_TEXT.length; i++) {
              var occ = t.split(BAD_TEXT[i]).length - 1;
              if (occ >= 3) { showBlocked('Contenido inapropiado'); return false; }
            }
            return true;
          }
          if (!checkUrl()) return;

          window.__filterCb = window.__filterCb || {};
          function callFilterTexts(texts) {
            try {
              if (window.FilterBridge && window.FilterBridge.filterTexts) {
                return new Promise(function (resolve) {
                  var id = 'ts' + Date.now() + '_' + Math.random().toString(36).slice(2);
                  window.__filterCb[id] = resolve;
                  window.FilterBridge.filterTexts(id, JSON.stringify(texts));
                });
              }
            } catch (_) {}
            return Promise.resolve(texts.slice());
          }
          function callFilterImage(url) {
            try {
              if (window.FilterBridge && window.FilterBridge.filterImage) {
                return new Promise(function (resolve) {
                  var id = 'i' + Date.now() + '_' + Math.random().toString(36).slice(2);
                  window.__filterCb[id] = resolve;
                  window.FilterBridge.filterImage(id, url);
                });
              }
            } catch (_) {}
            return Promise.resolve(url);
          }

          var imageCache = {}, imageInFlight = 0, imageQueue = [], IMG_CONC = 2;
          function dispatchImage() {
            while (imageInFlight < IMG_CONC && imageQueue.length > 0) {
              var job = imageQueue.shift();
              imageInFlight++;
              callFilterImage(job.url).then(function (out) { job.resolve(out); }, function () { job.resolve(job.url); }).then(function () { imageInFlight--; dispatchImage(); });
            }
          }
          function filterImageCached(url) {
            if (imageCache[url]) return imageCache[url];
            imageCache[url] = new Promise(function (resolve) { imageQueue.push({ url: url, resolve: resolve }); dispatchImage(); });
            return imageCache[url];
          }

          function reveal(el) { el.classList.add('__sb_done'); }
          function processImg(img) {
            if (!img || img.classList.contains('__sb_done')) return;
            var src = img.currentSrc || img.src || '';
            if (!src || src.indexOf('data:') === 0 || src.indexOf('blob:') === 0) { reveal(img); return; }
            var w = img.naturalWidth || img.width || 0;
            var h = img.naturalHeight || img.height || 0;
            if (w > 0 && h > 0 && (w < 64 || h < 64)) { reveal(img); return; }
            filterImageCached(src).then(function (out) {
              if (out && out !== src) { try { img.src = out; } catch (_) {} }
              reveal(img);
            }, function () { reveal(img); });
          }

          var imgObserver = null;
          try {
            imgObserver = new IntersectionObserver(function (entries) {
              for (var i = 0; i < entries.length; i++) {
                var e = entries[i];
                if (e.isIntersecting) { imgObserver.unobserve(e.target); processImg(e.target); }
              }
            }, { rootMargin: '200px' });
          } catch (_) { imgObserver = null; }
          function queueImg(img) {
            if (!img || img.classList.contains('__sb_done')) return;
            if (imgObserver) { try { imgObserver.observe(img); return; } catch (_) {} }
            processImg(img);
          }

          var TEXT_SELECTOR = 'p:not(.__sb_done),h1:not(.__sb_done),h2:not(.__sb_done),h3:not(.__sb_done),h4:not(.__sb_done),h5:not(.__sb_done),h6:not(.__sb_done)';
          function scanRoot(root) {
            if (!root || !root.querySelectorAll) return;
            try {
              var imgs = root.querySelectorAll('img:not(.__sb_done)');
              for (var j = 0; j < imgs.length; j++) queueImg(imgs[j]);
            } catch (_) {}
            var elems = [], texts = [];
            try {
              var nodes = root.querySelectorAll(TEXT_SELECTOR);
              for (var i = 0; i < nodes.length; i++) {
                var t = nodes[i].textContent || '';
                if (t.length < 3) { reveal(nodes[i]); continue; }
                elems.push(nodes[i]); texts.push(t);
              }
            } catch (_) {}
            if (texts.length === 0) return;
            callFilterTexts(texts).then(function (out) {
              for (var k = 0; k < elems.length; k++) {
                var v = (out && out[k] != null) ? out[k] : texts[k];
                try { elems[k].textContent = v; } catch (_) {}
                reveal(elems[k]);
              }
            }, function () {
              for (var k = 0; k < elems.length; k++) reveal(elems[k]);
            });
          }

          var scanScheduled = false;
          function scheduleScan() {
            if (scanScheduled) return;
            scanScheduled = true;
            setTimeout(function () { scanScheduled = false; scanRoot(document.body || document.documentElement); }, 50);
          }

          function onReady() {
            if (!checkText()) return;
            scanRoot(document.body);
            try {
              var obs = new MutationObserver(function (muts) {
                var needScan = false;
                for (var i = 0; i < muts.length; i++) {
                  if (muts[i].type === 'childList' && muts[i].addedNodes.length > 0) needScan = true;
                  if (muts[i].type === 'attributes' && muts[i].target && muts[i].target.tagName === 'IMG' && muts[i].attributeName === 'src') {
                    muts[i].target.classList.remove('__sb_done');
                    queueImg(muts[i].target);
                  }
                }
                if (needScan) scheduleScan();
              });
              obs.observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ['src'] });
              var blockObs = new MutationObserver(function () { if (!checkText()) blockObs.disconnect(); });
              blockObs.observe(document.body, { childList: true, subtree: true, characterData: true });
            } catch (_) {}
          }
          if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', onReady);
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

                // Bridge JS → Kotlin para los dos filtros. Debe registrarse antes
                // del primer loadUrl. Se reusa across navigations porque el WebView
                // se reusa.
                wv.addJavascriptInterface(FilterBridge(wv), "FilterBridge")

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
                it.removeJavascriptInterface("FilterBridge")
                it.webViewClient = WebViewClient()
                (it.parent as? ViewGroup)?.removeView(it)
                it.destroy()
            }
            webView = null
            invoke.resolve()
        }
    }

    /// Bridge JS → Kotlin. Los métodos @JavascriptInterface corren en una thread
    /// del WebView (no UI), así que el trabajo CPU/IO no bloquea la página.
    /// Resolución asíncrona: JS guarda un callback en window.__filterCb[id] y
    /// nosotros invocamos `evaluateJavascript` con el resultado JSON-encoded.
    inner class FilterBridge(private val wv: WebView) {
        // v2: batched. JS llama FilterBridge.filterTexts(id, JSON.stringify([...]))
        @JavascriptInterface
        fun filterTexts(id: String, textsJson: String) {
            // Parseo JSON inline (sin lib externa): un array de strings.
            val texts = parseJsonStringArray(textsJson)
            val outList = texts.map { filterTextNative(it) }
            replyToJsRaw(id, jsonStringArray(outList))
        }

        @JavascriptInterface
        fun filterImage(id: String, url: String) {
            // Fork a thread separada — el download + blur es lento y no debe
            // ocupar la binder thread del WebView.
            thread(start = true) {
                val out = try {
                    filterImageNative(url)
                } catch (_: Throwable) {
                    url
                }
                replyToJsRaw(id, jsonString(out))
            }
        }

        private fun replyToJsRaw(id: String, jsonExpr: String) {
            val safeId = jsonString(id)
            wv.post {
                wv.evaluateJavascript(
                    "(function(){var cb=window.__filterCb&&window.__filterCb[$safeId];if(cb){delete window.__filterCb[$safeId];cb($jsonExpr);}})()",
                    null
                )
            }
        }
    }

    /// Reemplaza ~30% de letras alfabéticas por '-'. Preserva espacios, dígitos
    /// y puntuación para que se lea como censura intencional.
    private fun filterTextNative(text: String): String {
        val sb = StringBuilder(text.length)
        for (ch in text) {
            if (ch.isLetter() && Random.nextDouble() < 0.30) sb.append('-') else sb.append(ch)
        }
        return sb.toString()
    }

    /// Baja la imagen, aplica un "blur fuerte" via downscale-upscale (compatible
    /// con todas las API levels desde minSdk 24, sin RenderScript). Encoda como
    /// JPEG y devuelve data URL.
    private fun filterImageNative(urlString: String): String {
        val url = URL(urlString)
        val conn = url.openConnection() as HttpURLConnection
        conn.connectTimeout = 8000
        conn.readTimeout = 8000
        conn.requestMethod = "GET"
        conn.instanceFollowRedirects = true
        try {
            conn.connect()
            if (conn.responseCode !in 200..299) return urlString
            val bytes = conn.inputStream.use { it.readBytes() }
            val src = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return urlString
            val blurred = heavyBlur(src)
            val out = ByteArrayOutputStream()
            blurred.compress(Bitmap.CompressFormat.JPEG, 75, out)
            blurred.recycle()
            if (src !== blurred) src.recycle()
            val b64 = Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP)
            return "data:image/jpeg;base64,$b64"
        } catch (_: IOException) {
            return urlString
        } finally {
            conn.disconnect()
        }
    }

    /// Equivalente a Gaussian blur fuerte: downscale brutal + upscale bilinear.
    /// Se ejecuta dos veces en cascada para suavizar más sin agregar deps.
    private fun heavyBlur(src: Bitmap): Bitmap {
        val maxDim = 256
        val sw = src.width.coerceAtLeast(1)
        val sh = src.height.coerceAtLeast(1)
        val displayScale = minOf(maxDim.toFloat() / sw, maxDim.toFloat() / sh, 1f)
        val displayW = (sw * displayScale).toInt().coerceAtLeast(1)
        val displayH = (sh * displayScale).toInt().coerceAtLeast(1)

        // Downscale a ~24px en el lado mayor — cualquier detalle se pierde.
        val tinyScale = 24f / maxOf(sw, sh).toFloat()
        val tinyW = (sw * tinyScale).toInt().coerceAtLeast(2)
        val tinyH = (sh * tinyScale).toInt().coerceAtLeast(2)

        val tiny = Bitmap.createScaledBitmap(src, tinyW, tinyH, true)
        val out = Bitmap.createBitmap(displayW, displayH, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(out)
        val paint = Paint(Paint.FILTER_BITMAP_FLAG or Paint.ANTI_ALIAS_FLAG)
        canvas.drawBitmap(
            tiny,
            Rect(0, 0, tinyW, tinyH),
            Rect(0, 0, displayW, displayH),
            paint
        )
        tiny.recycle()
        return out
    }

    /// JSON-string-escape minimalista para inyectar values al JS via
    /// evaluateJavascript. Maneja \, ", \n, \r, \t y caracteres de control.
    private fun jsonString(s: String): String {
        val sb = StringBuilder(s.length + 2)
        sb.append('"')
        for (ch in s) {
            when (ch) {
                '\\' -> sb.append("\\\\")
                '"' -> sb.append("\\\"")
                '\n' -> sb.append("\\n")
                '\r' -> sb.append("\\r")
                '\t' -> sb.append("\\t")
                '\b' -> sb.append("\\b")
                else -> {
                    if (ch.code < 0x20) {
                        sb.append(String.format("\\u%04x", ch.code))
                    } else {
                        sb.append(ch)
                    }
                }
            }
        }
        sb.append('"')
        return sb.toString()
    }

    /// Encoda una lista de strings como JSON array (e.g. `["a","b","c"]`).
    private fun jsonStringArray(items: List<String>): String {
        val sb = StringBuilder()
        sb.append('[')
        for ((i, s) in items.withIndex()) {
            if (i > 0) sb.append(',')
            sb.append(jsonString(s))
        }
        sb.append(']')
        return sb.toString()
    }

    /// Parsea un JSON array de strings. Devuelve lista vacía si malformado.
    private fun parseJsonStringArray(s: String): List<String> {
        return try {
            val arr = org.json.JSONArray(s)
            val out = ArrayList<String>(arr.length())
            for (i in 0 until arr.length()) out.add(arr.optString(i, ""))
            out
        } catch (_: Throwable) {
            emptyList()
        }
    }
}
