// Plugin nativo iOS: añade un WKWebView embebido como subview del
// UIViewController principal. Posicionado y dimensionado desde Rust/JS para
// que coincida con el placeholder paneRef en la React UI.

import Foundation
import SwiftRs
import Tauri
import UIKit
import WebKit

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

  static let userAgent =
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1"

  static let filterScript = """
  (function () {
    if (window.__sandboxFilterInstalled) return;
    window.__sandboxFilterInstalled = true;
    var BAD_TEXT = ["porn", "xxx", "nsfw"];
    function showBlocked() {
      try {
        document.open();
        document.write('<html><body style="font-family:-apple-system;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#fde68a;color:#7c2d12"><div style="text-align:center;padding:32px"><div style="font-size:48px">🚫</div><h1>Sitio bloqueado</h1><p>Este contenido no está permitido.</p></div></body></html>');
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
        var obs = new MutationObserver(function(){ if (!checkText()) obs.disconnect(); });
        obs.observe(document.body, { childList: true, subtree: true, characterData: true });
      } catch (_) {}
    }
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", onReady);
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

@_cdecl("init_plugin_native_browser_pane")
func initPlugin() -> Plugin {
  return NativeBrowserPanePlugin()
}
