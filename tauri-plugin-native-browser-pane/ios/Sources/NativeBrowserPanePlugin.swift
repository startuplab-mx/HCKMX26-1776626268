// Plugin nativo iOS: añade un WKWebView embebido como subview del
// UIViewController principal. Posicionado y dimensionado desde Rust/JS para
// que coincida con el placeholder paneRef en la React UI.

import CoreImage
import CoreImage.CIFilterBuiltins
import Foundation
import SwiftRs
import Tauri
import UIKit
import WebKit

// Rust FFI: clasifica un batch de textos vía el modelo zero-shot multi-hipótesis.
// Implementado en `tauri-plugin-native-browser-pane/src/ios_filter.rs`.
// Recibe el path del Bundle (donde viven runtime.json + onnx_model/) y un JSON
// array de strings; devuelve un JSON array de strings filtrados.
@_silgen_name("classifier_filter_texts")
func classifier_filter_texts(_ bundlePath: SRString, _ textsJson: SRString) -> SRString

// Rust FFI: clasifica una imagen (bytes) con MobileCLIP-S1 zero-shot.
// Implementado en `tauri-plugin-native-browser-pane/src/ios_filter.rs`.
// Devuelve "allow", "block", o "none" (modelo no cargado / decode falló).
// El handler Swift decide blurear o passthrough con base en el veredicto;
// la inferencia y el preprocess viven en Rust para compartir lógica con el
// path desktop (`filter_image_bytes` en app/src-tauri/src/lib.rs).
@_silgen_name("classifier_classify_image_bytes")
func classifier_classify_image_bytes(_ bytes: SRData) -> SRString

class OpenArgs: Decodable {
  let url: String
  let x: Double
  let y: Double
  let width: Double
  let height: Double
}

class BoundsArgs: Decodable {
  let x: Double
  let y: Double
  let width: Double
  let height: Double
}

class NavigateArgs: Decodable {
  let url: String
}

class NativeBrowserPanePlugin: Plugin {
  var webView: WKWebView?
  var navDelegate: NativeBrowserPaneNavDelegate?
  var filterHandler: FilterMessageHandler?

  static let userAgent =
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1"

  // El contenido de filter.js v2 — batched text + URL-based image (mobile path).
  // Mantener en sync conceptualmente con app/src-tauri/src/filter.js, pero la
  // versión iOS solo necesita las branches de webkit.messageHandlers.
  static let filterScript = """
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

    var BAD_URL_PATTERNS = [/porn/i,/xxx/i,/xvideos/i,/pornhub/i,/redtube/i,/youporn/i,/xnxx/i,/onlyfans/i,/chaturbate/i,/\\bnsfw\\b/i];
    var BAD_TEXT = ['porn','xxx','nsfw'];
    function showBlocked(reason) {
      try {
        document.open();
        document.write('<html><body style="font-family:-apple-system;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#fde68a;color:#7c2d12"><div style="text-align:center;padding:32px"><div style="font-size:48px">🚫</div><h1>Sitio bloqueado</h1><p>'+(reason||'')+'</p></div></body></html>');
        document.close();
      } catch (_) {}
    }
    function checkUrl() {
      var url = location.href || '';
      for (var i = 0; i < BAD_URL_PATTERNS.length; i++) {
        if (BAD_URL_PATTERNS[i].test(url)) { showBlocked('URL bloqueada'); return false; }
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

    // Bridges hacia Swift (WKScriptMessageHandlerWithReply en iOS 14+).
    function callFilterTexts(texts) {
      try {
        if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.filterTexts) {
          return window.webkit.messageHandlers.filterTexts.postMessage(texts);
        }
      } catch (_) {}
      return Promise.resolve(texts.slice());
    }
    function callFilterImage(url) {
      try {
        if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.filterImage) {
          return window.webkit.messageHandlers.filterImage.postMessage(url);
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

    // TreeWalker para cobertura ancha de texto (descripciones DDG, etc).
    var SKIP_TAGS = { SCRIPT:1,STYLE:1,NOSCRIPT:1,TEMPLATE:1,IFRAME:1,SVG:1,CANVAS:1,INPUT:1,TEXTAREA:1,SELECT:1,OPTION:1,BUTTON:1,LABEL:1,NAV:1,CODE:1,PRE:1,KBD:1,SAMP:1 };
    var SKIP_ROLES = { button:1,tab:1,menuitem:1,menuitemcheckbox:1,menuitemradio:1,switch:1,radio:1,checkbox:1,navigation:1,banner:1,contentinfo:1,search:1 };
    var processedTextNodes = new WeakSet();
    function shouldSkipForTextWalk(textNode) {
      var p = textNode.parentNode;
      while (p && p.nodeType === 1) {
        var tag = p.tagName;
        if (SKIP_TAGS[tag]) return true;
        if (p.isContentEditable) return true;
        if (p.hidden) return true;
        var aHidden = p.getAttribute && p.getAttribute('aria-hidden');
        if (aHidden === 'true') return true;
        var role = p.getAttribute && p.getAttribute('role');
        if (role && SKIP_ROLES[role.toLowerCase()]) return true;
        if (tag === 'BODY') break;
        p = p.parentNode;
      }
      return false;
    }
    var LETTER_SEQ;
    try { LETTER_SEQ = new RegExp('\\\\p{L}{4,}', 'u'); } catch (_) { LETTER_SEQ = /[A-Za-zÀ-ɏ]{4,}/; }
    function isContentText(text) {
      if (!text) return false;
      var trimmed = text.replace(/^\\s+|\\s+$/g, '');
      if (trimmed.length < 8) return false;
      if (!LETTER_SEQ.test(trimmed)) return false;
      return true;
    }
    var REVEAL_SELECTOR = 'p:not(.__sb_done),h1:not(.__sb_done),h2:not(.__sb_done),h3:not(.__sb_done),h4:not(.__sb_done),h5:not(.__sb_done),h6:not(.__sb_done)';

    function scanRoot(root) {
      if (!root) return;
      try {
        if (root.querySelectorAll) {
          var imgs = root.querySelectorAll('img:not(.__sb_done)');
          for (var j = 0; j < imgs.length; j++) queueImg(imgs[j]);
        }
      } catch (_) {}
      var candidates = [];
      try {
        var walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
        var node;
        while ((node = walker.nextNode())) {
          if (processedTextNodes.has(node)) continue;
          if (!isContentText(node.nodeValue)) { processedTextNodes.add(node); continue; }
          if (shouldSkipForTextWalk(node)) { processedTextNodes.add(node); continue; }
          candidates.push(node);
        }
      } catch (_) {}
      var revealEls = [];
      try {
        if (root.querySelectorAll) {
          var rs = root.querySelectorAll(REVEAL_SELECTOR);
          for (var r = 0; r < rs.length; r++) revealEls.push(rs[r]);
        }
      } catch (_) {}
      if (candidates.length === 0) {
        for (var i = 0; i < revealEls.length; i++) reveal(revealEls[i]);
        return;
      }
      var texts = [];
      for (var c = 0; c < candidates.length; c++) texts.push(candidates[c].nodeValue);
      callFilterTexts(texts).then(function (out) {
        for (var k = 0; k < candidates.length; k++) {
          var v = (out && out[k] != null) ? out[k] : texts[k];
          try { candidates[k].nodeValue = v; } catch (_) {}
          processedTextNodes.add(candidates[k]);
        }
        for (var rv = 0; rv < revealEls.length; rv++) reveal(revealEls[rv]);
      }, function () {
        for (var m = 0; m < candidates.length; m++) processedTextNodes.add(candidates[m]);
        for (var rv2 = 0; rv2 < revealEls.length; rv2++) reveal(revealEls[rv2]);
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
  """

  @objc public func open(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(OpenArgs.self)
    DispatchQueue.main.async {
      let requested = CGRect(x: args.x, y: args.y, width: args.width, height: args.height)

      if let existing = self.webView {
        if let parent = existing.superview {
          existing.frame = NativeBrowserPanePlugin.adjustedFrame(requested, in: parent)
        } else {
          existing.frame = requested
        }
        if let url = URL(string: args.url) {
          existing.load(URLRequest(url: url))
        }
        invoke.resolve()
        return
      }

      let config = WKWebViewConfiguration()
      config.websiteDataStore = .default()
      let userScript = WKUserScript(
        source: NativeBrowserPanePlugin.filterScript,
        injectionTime: .atDocumentStart,
        forMainFrameOnly: false
      )
      config.userContentController.addUserScript(userScript)

      // Bridge JS → Swift para los dos filtros (iOS 14+). v2: filterTexts
      // (batched) reemplaza al per-element filterText. filterImage queda igual.
      let handler = FilterMessageHandler()
      config.userContentController.addScriptMessageHandler(
        handler, contentWorld: .page, name: "filterTexts")
      config.userContentController.addScriptMessageHandler(
        handler, contentWorld: .page, name: "filterImage")
      self.filterHandler = handler

      let webView = WKWebView(frame: requested, configuration: config)
      webView.customUserAgent = NativeBrowserPanePlugin.userAgent
      webView.allowsBackForwardNavigationGestures = true
      webView.translatesAutoresizingMaskIntoConstraints = true
      webView.autoresizingMask = []
      webView.clipsToBounds = true
      webView.layer.cornerRadius = 12
      webView.layer.masksToBounds = true
      webView.backgroundColor = UIColor.white
      webView.isOpaque = true
      webView.scrollView.backgroundColor = UIColor.white

      let delegate = NativeBrowserPaneNavDelegate(plugin: self)
      webView.navigationDelegate = delegate
      self.navDelegate = delegate

      if let rootVC = self.manager.viewController as UIViewController? {
        let parentView: UIView =
          NativeBrowserPanePlugin.findReactWebView(in: rootVC.view, exclude: webView)
            ?? rootVC.view
        parentView.addSubview(webView)
        webView.frame = NativeBrowserPanePlugin.adjustedFrame(requested, in: parentView)
      }

      if let url = URL(string: args.url) {
        webView.load(URLRequest(url: url))
      }
      self.webView = webView
      invoke.resolve()
    }
  }

  @objc public func setBounds(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(BoundsArgs.self)
    DispatchQueue.main.async {
      let requested = CGRect(
        x: args.x, y: args.y, width: args.width, height: args.height
      )
      if let wv = self.webView, let parent = wv.superview {
        wv.frame = NativeBrowserPanePlugin.adjustedFrame(requested, in: parent)
      } else {
        self.webView?.frame = requested
      }
      invoke.resolve()
    }
  }

  @objc public func navigate(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(NavigateArgs.self)
    DispatchQueue.main.async {
      if let url = URL(string: args.url) {
        self.webView?.load(URLRequest(url: url))
      }
      invoke.resolve()
    }
  }

  @objc public func close(_ invoke: Invoke) throws {
    DispatchQueue.main.async {
      if let wv = self.webView {
        wv.isHidden = true
        wv.stopLoading()
        wv.navigationDelegate = nil
        wv.removeFromSuperview()
      }
      self.webView = nil
      self.navDelegate = nil
      self.filterHandler = nil
      invoke.resolve()
    }
  }

  /// Walk recursivo del árbol de UIViews buscando un WKWebView (que será el
  /// principal de Tauri donde corre React). `exclude` permite saltarse el
  /// nuestro si se llama post-creación.
  static func findReactWebView(in view: UIView, exclude: WKWebView?) -> WKWebView? {
    if let wv = view as? WKWebView, wv !== exclude {
      return wv
    }
    for sub in view.subviews {
      if let found = findReactWebView(in: sub, exclude: exclude) {
        return found
      }
    }
    return nil
  }

  /// Recorta `requested` para que no se salga de los bounds del parent.
  /// Garantiza que la WKWebView embebida quede dentro del área visible y por
  /// tanto sus esquinas redondeadas se vean en los 4 lados.
  static func clampToParent(_ requested: CGRect, parent: CGRect) -> CGRect {
    let x = max(0, min(requested.origin.x, parent.width))
    let y = max(0, min(requested.origin.y, parent.height))
    let maxW = max(0, parent.width - x)
    let maxH = max(0, parent.height - y)
    let w = min(requested.size.width, maxW)
    let h = min(requested.size.height, maxH)
    return CGRect(x: x, y: y, width: w, height: h)
  }

  /// Convierte coords del HTML viewport (lo que da `getBoundingClientRect` en
  /// React) a coords del bounds del WKWebView padre.
  ///
  /// Con `viewport-fit=cover` en el `<meta viewport>`, el HTML viewport cubre
  /// el full WKWebView frame (sin auto-inset) y React respeta safe area via
  /// `env(safe-area-inset-*)` en CSS padding. Por tanto las coords reportadas
  /// por `getBoundingClientRect` ya incluyen el offset del status bar — no
  /// hay que sumar el `adjustedContentInset` (que duplicaría el offset).
  static func adjustedFrame(_ requested: CGRect, in parent: UIView) -> CGRect {
    return clampToParent(requested, parent: parent.bounds)
  }
}

class NativeBrowserPaneNavDelegate: NSObject, WKNavigationDelegate {
  weak var plugin: NativeBrowserPanePlugin?

  init(plugin: NativeBrowserPanePlugin) {
    self.plugin = plugin
    super.init()
  }

  static let badPatterns: [String] = [
    "porn", "xxx", "xvideos", "pornhub", "redtube", "youporn",
    "xnxx", "onlyfans", "chaturbate"
  ]

  func webView(
    _ webView: WKWebView,
    decidePolicyFor navigationAction: WKNavigationAction,
    decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
  ) {
    let url = navigationAction.request.url?.absoluteString.lowercased() ?? ""
    if Self.badPatterns.contains(where: { url.contains($0) }) {
      plugin?.trigger("browser-blocked", data: ["url": url])
      decisionHandler(.cancel)
      return
    }
    if !url.isEmpty {
      plugin?.trigger("browser-navigated", data: ["url": url])
    }
    decisionHandler(.allow)
  }
}

/// Bridge JS → Swift para las dos funciones de filtro. Usa
/// `WKScriptMessageHandlerWithReply` (iOS 14+) que devuelve Promises a JS.
///
/// Implementación con GCD + URLSession completion handler, NO Swift
/// Concurrency: la combinación `Task { try await URLSession.shared.data(from:) }`
/// crashea en swift_task_alloc en iOS 17 simulator (EXC_BAD_ACCESS dentro del
/// runtime de Swift Concurrency cuando el Task se crea desde el callback ObjC
/// de WKScriptMessageHandler).
class FilterMessageHandler: NSObject, WKScriptMessageHandlerWithReply {
  static let ciContext = CIContext(options: nil)
  // Serial: la Session ONNX en Rust ya está bajo Mutex. Permitir concurrencia
  // aquí solo apila trabajo redundante en la cola del lock y multiplica los
  // batches FFI (4 hilos disparando el mismo batch idéntico).
  static let workQueue = DispatchQueue(
    label: "com.hackathon404.filter.work", qos: .userInitiated)

  func userContentController(
    _ userContentController: WKUserContentController,
    didReceive message: WKScriptMessage,
    replyHandler: @escaping (Any?, String?) -> Void
  ) {
    switch message.name {
    case "filterTexts":
      // El JS hace postMessage(arrayOfStrings) → llega como NSArray<NSString>.
      let texts = (message.body as? [String]) ?? []
      FilterMessageHandler.workQueue.async {
        let out = FilterMessageHandler.filterTexts(texts)
        replyHandler(out, nil)
      }
    case "filterImage":
      let urlString = (message.body as? String) ?? ""
      FilterMessageHandler.filterImage(urlString) { out in
        replyHandler(out, nil)
      }
    default:
      replyHandler(nil, "unknown handler: \(message.name)")
    }
  }

  /// Versión batched: delega al classifier zero-shot (Rust) vía FFI swift-rs.
  /// JSON encode/decode evita la limitación de SRArray no-construible.
  /// Si el modelo no está cargado en Rust, la función Rust devuelve los textos
  /// sin cambios, así que esto es seguro como passthrough.
  static func filterTexts(_ texts: [String]) -> [String] {
    let bundlePath = Bundle.main.resourcePath ?? ""
    guard
      let inputData = try? JSONSerialization.data(withJSONObject: texts),
      let inputJson = String(data: inputData, encoding: .utf8)
    else {
      return texts
    }

    let resultSR = classifier_filter_texts(SRString(bundlePath), SRString(inputJson))
    let resultStr = resultSR.toString()

    guard
      let resultData = resultStr.data(using: .utf8),
      let resultArr = try? JSONSerialization.jsonObject(with: resultData) as? [String]
    else {
      return texts
    }
    return resultArr
  }

  /// Baja la imagen, la pasa por el classifier MobileCLIP en Rust, y según
  /// el veredicto: (a) "allow" → devuelve el URL original (el WebView ya
  /// tiene los bytes cacheados, JS detecta out===src y solo levanta el
  /// CSS pre-hide), (b) "block"/"none" → aplica CIGaussianBlur y devuelve
  /// data URL JPEG. Veredicto "none" cubre tanto modelo no cargado como
  /// decode error: fail-closed (blur) por seguridad, mismo contrato que
  /// `filter_image_bytes` en desktop.
  ///
  /// Completion puede invocarse desde cualquier thread; el caller
  /// (WKScriptMessageHandlerWithReply) acepta replies desde cualquier queue.
  static func filterImage(_ urlString: String, completion: @escaping (String) -> Void) {
    guard let url = URL(string: urlString) else {
      completion(urlString)
      return
    }
    let task = URLSession.shared.dataTask(with: url) { data, _, error in
      guard let data = data, error == nil else {
        completion(urlString)
        return
      }
      // Salir del callback del URLSession antes de hacer trabajo CPU pesado:
      // la queue interna del URLSession es compartida con otros downloads.
      FilterMessageHandler.workQueue.async {
        let srData = SRData([UInt8](data))
        let verdict = classifier_classify_image_bytes(srData).toString()
        switch verdict {
        case "allow":
          // Imagen benigna: devolver el URL original tal cual. El WebView
          // ya tiene los bytes cacheados — no hay re-fetch.
          completion(urlString)
        default:
          // "block" o "none" (modelo no cargado / decode falló) →
          // fail-closed: aplicamos blur. Mismo comportamiento conservador
          // que `filter_image_bytes` desktop cuando el classifier es None.
          let result = FilterMessageHandler.blurImageData(data) ?? urlString
          completion(result)
        }
      }
    }
    task.resume()
  }

  /// Decodifica bytes → CIImage → Gaussian blur → JPEG → data URL.
  /// Síncrono, llama desde una background queue.
  private static func blurImageData(_ data: Data) -> String? {
    guard let inputImage = CIImage(data: data) else { return nil }
    let blurFilter = CIFilter.gaussianBlur()
    blurFilter.inputImage = inputImage
    blurFilter.radius = 15.0
    guard let outputImage = blurFilter.outputImage else { return nil }
    // El blur extiende los bounds; recorta al rectángulo original.
    let cropped = outputImage.cropped(to: inputImage.extent)
    guard let cgImage = ciContext.createCGImage(cropped, from: inputImage.extent) else {
      return nil
    }
    let uiImage = UIImage(cgImage: cgImage)
    guard let jpeg = uiImage.jpegData(compressionQuality: 0.75) else { return nil }
    return "data:image/jpeg;base64,\(jpeg.base64EncodedString())"
  }
}

@_cdecl("init_plugin_native_browser_pane")
func initPlugin() -> Plugin {
  return NativeBrowserPanePlugin()
}
