// Content filter inyectado en cada navegación de la WebView de "browser_pane".
// Corre antes que cualquier script de la página gracias a `initialization_script`
// (desktop) o `WKUserScript`/`evaluateJavascript` at-document-start (mobile).
//
// v2 — performance pass:
//   - Imagen: el JS hace fetch (cache hit del browser) y manda los bytes raw
//     vía `invoke('filter_image_bytes', { bytes: Uint8Array })`. Rust devuelve
//     bytes JPEG via `tauri::ipc::Response`. Cero base64, cero double-download.
//     En mobile (sin __TAURI_INTERNALS__) cae a la API URL legacy.
//   - Texto: se procesan en batch — un solo invoke por scan. De N IPCs a 1.
//   - Imágenes off-screen quedan con CSS blur (instant, gratis) y se procesan
//     vía IntersectionObserver al entrar al viewport.
//   - Si fetch falla (CORS), el CSS blur del pre-hide queda permanente. Cumple
//     "siempre difumina" sin depender de la pipeline.
(function () {
  if (window.__sandboxFilterInstalled) return;
  window.__sandboxFilterInstalled = true;

  // ---------- 1. Pre-hide CSS (corre antes de que exista <head>) ----------
  try {
    var pre = document.createElement("style");
    pre.id = "__sandbox_pre";
    pre.textContent =
      "p:not(.__sb_done),h1:not(.__sb_done),h2:not(.__sb_done)," +
      "h3:not(.__sb_done),h4:not(.__sb_done),h5:not(.__sb_done)," +
      "h6:not(.__sb_done){color:transparent !important;text-shadow:none !important;}" +
      "img:not(.__sb_done){filter:blur(24px) !important;}";
    (document.head || document.documentElement).appendChild(pre);
  } catch (_) {}

  // ---------- 2. URL/keyword block existente ----------
  var BAD_URL_PATTERNS = [
    /porn/i, /xxx/i, /xvideos/i, /pornhub/i, /redtube/i, /youporn/i, /xnxx/i,
    /onlyfans/i, /chaturbate/i, /\bnsfw\b/i,
  ];
  var BAD_TEXT_KEYWORDS = ["porn", "xxx", "nsfw"];

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
      var occurrences = txt.split(BAD_TEXT_KEYWORDS[i]).length - 1;
      if (occurrences >= 3) {
        showBlocked("Contenido inapropiado detectado");
        return false;
      }
    }
    return true;
  }

  if (!checkUrl()) return;

  // ---------- 3. Bridges multiplataforma ----------

  // TEXTO BATCHED — desktop: invoke nativo. Mobile: bridge nativo (filterTexts).
  function callFilterTexts(texts) {
    try {
      if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke) {
        return window.__TAURI_INTERNALS__.invoke("filter_texts", { texts: texts });
      }
      if (
        window.webkit &&
        window.webkit.messageHandlers &&
        window.webkit.messageHandlers.filterTexts
      ) {
        // iOS WKScriptMessageHandlerWithReply — el handler nativo procesa el
        // array completo y devuelve [String].
        return window.webkit.messageHandlers.filterTexts.postMessage(texts);
      }
      if (window.FilterBridge && window.FilterBridge.filterTexts) {
        return new Promise(function (resolve) {
          var id = "ts" + Date.now() + "_" + Math.random().toString(36).slice(2);
          (window.__filterCb = window.__filterCb || {})[id] = resolve;
          window.FilterBridge.filterTexts(id, JSON.stringify(texts));
        });
      }
    } catch (_) {}
    return Promise.resolve(texts.slice());
  }

  // IMAGEN URL — fallback mobile (CIFilter en iOS, heavyBlur en Android).
  function callFilterImageUrl(url) {
    try {
      if (
        window.webkit &&
        window.webkit.messageHandlers &&
        window.webkit.messageHandlers.filterImage
      ) {
        return window.webkit.messageHandlers.filterImage.postMessage(url);
      }
      if (window.FilterBridge && window.FilterBridge.filterImage) {
        return new Promise(function (resolve) {
          var id = "i" + Date.now() + "_" + Math.random().toString(36).slice(2);
          (window.__filterCb = window.__filterCb || {})[id] = resolve;
          window.FilterBridge.filterImage(id, url);
        });
      }
    } catch (_) {}
    return Promise.resolve(url);
  }

  // ---------- 4. Cap de concurrencia para imágenes ----------
  var imageCache = {}; // url -> Promise<{ src, blobUrl? }>
  var imageInFlight = 0;
  var imageQueue = [];
  var IMG_CONCURRENCY = 2;
  var blobUrls = new Set();

  function dispatchImage() {
    while (imageInFlight < IMG_CONCURRENCY && imageQueue.length > 0) {
      var job = imageQueue.shift();
      imageInFlight++;
      job.run().then(
        function () { imageInFlight--; dispatchImage(); },
        function () { imageInFlight--; dispatchImage(); }
      );
    }
  }

  function enqueueImageJob(runFn) {
    return new Promise(function (resolve) {
      imageQueue.push({
        run: function () {
          return Promise.resolve()
            .then(runFn)
            .then(function (v) { resolve(v); }, function (e) { resolve(null); });
        },
      });
      dispatchImage();
    });
  }

  // ---------- 5. Procesamiento de imagen ----------
  function reveal(el) { el.classList.add("__sb_done"); }

  // Path desktop: fetch del browser (cache hit) → bytes raw → invoke binario →
  // bytes blurred → blob URL.
  async function processImgDesktop(img, src) {
    if (imageCache[src]) {
      var cached = await imageCache[src];
      if (cached) {
        try { img.src = cached; } catch (_) {}
        reveal(img);
      }
      return;
    }
    var promise = enqueueImageJob(async function () {
      var r;
      try {
        r = await fetch(src);
      } catch (_) {
        return null; // CORS / network → CSS blur queda permanente
      }
      if (!r || !r.ok) return null;
      var buf = await r.arrayBuffer();
      var u8 = new Uint8Array(buf);
      var blurredResp = await window.__TAURI_INTERNALS__.invoke(
        "filter_image_bytes",
        { bytes: u8 }
      );
      // Tauri 2 entrega Response como ArrayBuffer (o Uint8Array, según versión).
      var blob = new Blob([blurredResp], { type: "image/jpeg" });
      var blobUrl = URL.createObjectURL(blob);
      blobUrls.add(blobUrl);
      return blobUrl;
    });
    imageCache[src] = promise;
    var url = await promise;
    if (url) {
      try { img.src = url; } catch (_) {}
      reveal(img);
    }
    // Si url es null (fetch falló), CSS blur permanente. No reveal.
  }

  // Path mobile: bridge nativo con URL.
  async function processImgMobile(img, src) {
    if (imageCache[src]) {
      var cached = await imageCache[src];
      if (cached && cached !== src) {
        try { img.src = cached; } catch (_) {}
      }
      reveal(img);
      return;
    }
    var promise = enqueueImageJob(async function () {
      var out = await callFilterImageUrl(src);
      return out;
    });
    imageCache[src] = promise;
    var out = await promise;
    if (out && out !== src) {
      try { img.src = out; } catch (_) {}
    }
    reveal(img);
  }

  function processImg(img) {
    if (!img || img.classList.contains("__sb_done")) return;
    var src = img.currentSrc || img.src || "";
    if (!src || src.indexOf("data:") === 0 || src.indexOf("blob:") === 0) {
      reveal(img);
      return;
    }
    var w = img.naturalWidth || img.width || 0;
    var h = img.naturalHeight || img.height || 0;
    if (w > 0 && h > 0 && (w < 64 || h < 64)) {
      reveal(img);
      return;
    }
    if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke) {
      processImgDesktop(img, src).catch(function () {});
    } else {
      processImgMobile(img, src).catch(function () {});
    }
  }

  // ---------- 6. IntersectionObserver para imágenes lazy ----------
  var imgObserver = null;
  try {
    imgObserver = new IntersectionObserver(
      function (entries) {
        for (var i = 0; i < entries.length; i++) {
          var e = entries[i];
          if (e.isIntersecting) {
            imgObserver.unobserve(e.target);
            processImg(e.target);
          }
        }
      },
      { rootMargin: "200px" }
    );
  } catch (_) {
    imgObserver = null;
  }

  function queueImg(img) {
    if (!img || img.classList.contains("__sb_done")) return;
    if (imgObserver) {
      try { imgObserver.observe(img); return; } catch (_) {}
    }
    processImg(img);
  }

  // ---------- 7. Scan + batch de texto (TreeWalker sobre text nodes) ----------
  // Cobertura ancha: cualquier text node con texto sustancial. El narrow
  // selector p/h1-h6 del v2 perdía descripciones en <span>/<div> (DDG, news
  // sites, etc). Ahora caminamos text nodes y excluimos UI por ancestor tag/
  // role. Al replazar nodeValue (no textContent) preservamos HTML anidado
  // (<em>, <strong>, <a> dentro de un párrafo, etc).

  var SKIP_TAGS = {
    SCRIPT: 1, STYLE: 1, NOSCRIPT: 1, TEMPLATE: 1, IFRAME: 1, SVG: 1, CANVAS: 1,
    INPUT: 1, TEXTAREA: 1, SELECT: 1, OPTION: 1,
    BUTTON: 1, LABEL: 1, NAV: 1,
    CODE: 1, PRE: 1, KBD: 1, SAMP: 1,
  };
  var SKIP_ROLES = {
    button: 1, tab: 1, menuitem: 1, menuitemcheckbox: 1, menuitemradio: 1,
    switch: 1, radio: 1, checkbox: 1,
    navigation: 1, banner: 1, contentinfo: 1, search: 1,
  };

  var processedTextNodes = new WeakSet();

  function shouldSkipForTextWalk(textNode) {
    var p = textNode.parentNode;
    while (p && p.nodeType === 1) {
      var tag = p.tagName;
      if (SKIP_TAGS[tag]) return true;
      if (p.isContentEditable) return true;
      if (p.hidden) return true;
      var aHidden = p.getAttribute && p.getAttribute("aria-hidden");
      if (aHidden === "true") return true;
      var role = p.getAttribute && p.getAttribute("role");
      if (role && SKIP_ROLES[role.toLowerCase()]) return true;
      if (tag === "BODY") break;
      p = p.parentNode;
    }
    return false;
  }

  // Texto candidato: ≥8 chars trimmed Y secuencia ≥4 letras Unicode.
  // Filtra labels cortos ("OK", "Home"), números ("2024"), símbolos ("→ →").
  var LETTER_SEQ;
  try { LETTER_SEQ = new RegExp("\\p{L}{4,}", "u"); }
  catch (_) { LETTER_SEQ = /[A-Za-zÀ-ɏ]{4,}/; }

  // URL / dominio + path: cosas como "www.meta.com/es-la/facebook-app/" o
  // "play.google.com/store/apps/details?id=com.facebook." dominaban el
  // input al clasificador en navegación tipo DDG/Google. Nunca son
  // conversación y al modelo NLI le confunden mucho (suben los scores en
  // todas las cats por compartir vocabulario).
  var URL_PREFIX = /^\s*(https?:\/\/|www\.)/i;
  var DOMAIN_PATH = /\b[\w-]+\.(com|mx|es|org|net|io|app|co|gov|edu|info|wiki(pedia)?\.org)\b\/[\w\-./?=&%#+]*/i;

  function isContentText(text) {
    if (!text) return false;
    var trimmed = text.replace(/^\s+|\s+$/g, "");
    if (trimmed.length < 8) return false;
    if (!LETTER_SEQ.test(trimmed)) return false;
    if (URL_PREFIX.test(trimmed)) return false;
    // Si el trimmed es predominantemente "domain.tld/path", es navegación.
    if (DOMAIN_PATH.test(trimmed)) {
      var letters = (trimmed.match(/[A-Za-zÀ-ɏÀ-ɏ一-鿿]/g) || []).length;
      var sep = (trimmed.match(/[./?&=#]/g) || []).length;
      // Heurística: muchos separadores vs letras → URL/path con ruido.
      if (sep * 6 >= letters) return false;
    }
    return true;
  }

  // Pre-hide CSS sigue aplicando a p/h1-h6. Revealemos esos elementos después
  // del batch (con o sin candidatos dentro) para que no queden transparentes.
  var REVEAL_SELECTOR =
    "p:not(.__sb_done),h1:not(.__sb_done),h2:not(.__sb_done)," +
    "h3:not(.__sb_done),h4:not(.__sb_done),h5:not(.__sb_done),h6:not(.__sb_done)";

  async function scanRoot(root) {
    if (!root) return;
    // Imágenes → cola lazy (sin cambios)
    try {
      if (root.querySelectorAll) {
        var imgs = root.querySelectorAll("img:not(.__sb_done)");
        for (var j = 0; j < imgs.length; j++) queueImg(imgs[j]);
      }
    } catch (_) {}

    // Walk text nodes
    var candidates = [];
    try {
      var walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
      var node;
      while ((node = walker.nextNode())) {
        if (processedTextNodes.has(node)) continue;
        if (!isContentText(node.nodeValue)) {
          processedTextNodes.add(node);
          continue;
        }
        if (shouldSkipForTextWalk(node)) {
          processedTextNodes.add(node);
          continue;
        }
        candidates.push(node);
      }
    } catch (_) {}

    // Elementos pre-ocultos por CSS que hay que revelar (independiente de
    // si su texto fue filtrado o no — para no dejarlos transparentes).
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

    // Streaming chunks: trocea candidates en grupos pequeños y aplica los
    // resultados al DOM por cada chunk completado. El usuario ve la página
    // ir limpiándose progresivamente (cada ~1-2 s) en vez de un bloqueo
    // monolítico de 15-20 s al final del batch grande.
    var STREAM_CHUNK = 10;

    function revealAncestorBlock(node) {
      var p = node.parentNode;
      while (p && p.nodeType === 1) {
        var tag = p.tagName;
        if (tag === "P" || (tag && tag.length === 2 && tag.charAt(0) === "H" &&
            tag.charAt(1) >= "1" && tag.charAt(1) <= "6")) {
          reveal(p);
          return;
        }
        if (tag === "BODY") return;
        p = p.parentNode;
      }
    }

    for (var start = 0; start < candidates.length; start += STREAM_CHUNK) {
      var slice = candidates.slice(start, start + STREAM_CHUNK);
      var sliceTexts = [];
      for (var c = 0; c < slice.length; c++) sliceTexts.push(slice[c].nodeValue);

      try {
        var out = await callFilterTexts(sliceTexts);
        for (var k = 0; k < slice.length; k++) {
          var v = (out && out[k] != null) ? out[k] : sliceTexts[k];
          try { slice[k].nodeValue = v; } catch (_) {}
          processedTextNodes.add(slice[k]);
          // Revelar el bloque padre apenas tenemos su texto filtrado, en vez
          // de esperar al final del scan completo.
          revealAncestorBlock(slice[k]);
        }
      } catch (_) {
        for (var m = 0; m < slice.length; m++) processedTextNodes.add(slice[m]);
      }
    }
    // Limpieza final: cualquier p/h1-h6 que no haya tenido text node candidato
    // (vacío, solo punctuación, etc.) sigue transparente — revelarlo.
    for (var rv = 0; rv < revealEls.length; rv++) reveal(revealEls[rv]);
  }

  // Debounce + lock anti-concurrente. Mientras un scan corre, los triggers
  // adicionales solo marcan `scanDirty`; al terminar se vuelve a agendar UN
  // scan. Garantiza ≤1 scanRoot a la vez (filtra el ruido de SPAs como
  // Telegram Web que mutan en ráfagas y antes generaban 4 batches paralelos).
  var scanTimer = null;
  var scanRunning = false;
  var scanDirty = false;
  var SCAN_DEBOUNCE_MS = 150;

  function scheduleScan() {
    if (scanRunning) {
      scanDirty = true;
      return;
    }
    if (scanTimer != null) clearTimeout(scanTimer);
    scanTimer = setTimeout(runScan, SCAN_DEBOUNCE_MS);
  }

  async function runScan() {
    scanTimer = null;
    scanRunning = true;
    scanDirty = false;
    try {
      await scanRoot(document.body || document.documentElement);
    } catch (_) {
    } finally {
      scanRunning = false;
      if (scanDirty) scheduleScan();
    }
  }

  // ---------- 8. Cleanup blob URLs en pagehide ----------
  window.addEventListener("pagehide", function () {
    blobUrls.forEach(function (u) {
      try { URL.revokeObjectURL(u); } catch (_) {}
    });
    blobUrls.clear();
  });

  // ---------- 9. onReady + observers ----------
  function onReady() {
    if (!checkText()) return;

    scheduleScan();

    try {
      var contentObs = new MutationObserver(function (muts) {
        var needScan = false;
        for (var i = 0; i < muts.length; i++) {
          if (muts[i].type === "childList" && muts[i].addedNodes.length > 0) {
            needScan = true;
          }
          // <img src> mutado in-place (SPAs reusan nodos)
          if (
            muts[i].type === "attributes" &&
            muts[i].target &&
            muts[i].target.tagName === "IMG" &&
            muts[i].attributeName === "src"
          ) {
            muts[i].target.classList.remove("__sb_done");
            queueImg(muts[i].target);
          }
        }
        if (needScan) scheduleScan();
      });
      contentObs.observe(document.documentElement, {
        childList: true,
        subtree: true,
        attributes: true,
        attributeFilter: ["src"],
      });

      var blockObs = new MutationObserver(function () {
        if (!checkText()) blockObs.disconnect();
      });
      blockObs.observe(document.body, {
        childList: true,
        subtree: true,
        characterData: true,
      });
    } catch (_) {}
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", onReady);
  } else {
    onReady();
  }
})();
