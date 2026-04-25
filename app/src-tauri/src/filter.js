// Content filter inyectado en cada navegación de la WebView de "browser_pane".
// Corre antes que cualquier script de la página gracias a `initialization_script`.
(function () {
  if (window.__sandboxFilterInstalled) return;
  window.__sandboxFilterInstalled = true;

  var BAD_URL_PATTERNS = [
    /porn/i, /xxx/i, /xvideos/i, /pornhub/i, /redtube/i, /youporn/i, /xnxx/i,
    /onlyfans/i, /chaturbate/i, /\bnsfw\b/i,
  ];

  var BAD_TEXT_KEYWORDS = [
    "porn", "xxx", "nsfw",
  ];

  function showBlocked(reason) {
    try {
      var html =
        '<!doctype html><html><head><meta charset="utf-8"><title>Bloqueado</title></head>' +
        '<body style="font-family:-apple-system,BlinkMacSystemFont,system-ui,sans-serif;' +
        'display:flex;align-items:center;justify-content:center;height:100vh;margin:0;' +
        'background:linear-gradient(135deg,#fde68a,#fca5a5);color:#7c2d12;text-align:center">' +
        '<div style="padding:32px;max-width:380px">' +
        '<div style="font-size:48px;margin-bottom:8px">🚫</div>' +
        '<h1 style="margin:0 0 8px;font-size:24px">Sitio bloqueado</h1>' +
        '<p style="margin:0;color:#9a3412">Este contenido no está permitido en el navegador seguro.</p>' +
        '<p style="margin:8px 0 0;color:#9a3412;font-size:13px;opacity:0.7">' +
        (reason || "") + "</p>" +
        "</div></body></html>";
      document.open();
      document.write(html);
      document.close();
    } catch (_) {}
  }

  function checkUrl() {
    var url = location.href || "";
    for (var i = 0; i < BAD_URL_PATTERNS.length; i++) {
      if (BAD_URL_PATTERNS[i].test(url)) {
        showBlocked("URL bloqueada");
        return false;
      }
    }
    return true;
  }

  function checkText() {
    var body = document.body;
    if (!body) return true;
    var txt = (body.innerText || "").toLowerCase();
    if (txt.length < 50) return true;
    for (var i = 0; i < BAD_TEXT_KEYWORDS.length; i++) {
      var kw = BAD_TEXT_KEYWORDS[i];
      // Heurística simple: si la palabra aparece muchas veces, bloquear.
      var occurrences = txt.split(kw).length - 1;
      if (occurrences >= 3) {
        showBlocked("Contenido inapropiado detectado");
        return false;
      }
    }
    return true;
  }

  // Pre-render: verifica URL antes de mostrar nada.
  if (!checkUrl()) return;

  // Después del DOM ready: verifica el texto y monta observador.
  function onReady() {
    if (!checkText()) return;
    try {
      var obs = new MutationObserver(function () {
        if (!checkText()) {
          obs.disconnect();
        }
      });
      obs.observe(document.body, { childList: true, subtree: true, characterData: true });
    } catch (_) {}
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", onReady);
  } else {
    onReady();
  }
})();
