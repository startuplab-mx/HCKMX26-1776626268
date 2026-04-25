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

  // ---------- 7. Scan + batch de texto ----------
  var TEXT_SELECTOR =
    "p:not(.__sb_done),h1:not(.__sb_done),h2:not(.__sb_done)," +
    "h3:not(.__sb_done),h4:not(.__sb_done),h5:not(.__sb_done),h6:not(.__sb_done)";

  async function scanRoot(root) {
    if (!root || !root.querySelectorAll) return;
    // Imágenes → cola lazy
    try {
      var imgs = root.querySelectorAll("img:not(.__sb_done)");
      for (var j = 0; j < imgs.length; j++) queueImg(imgs[j]);
    } catch (_) {}

    // Textos → batch único
    var elems = [], texts = [];
    try {
      var nodes = root.querySelectorAll(TEXT_SELECTOR);
      for (var i = 0; i < nodes.length; i++) {
        var t = nodes[i].textContent || "";
        if (t.length < 3) { reveal(nodes[i]); continue; }
        elems.push(nodes[i]);
        texts.push(t);
      }
    } catch (_) {}
    if (texts.length === 0) return;

    try {
      var out = await callFilterTexts(texts);
      for (var k = 0; k < elems.length; k++) {
        var v = (out && out[k] != null) ? out[k] : texts[k];
        try { elems[k].textContent = v; } catch (_) {}
        reveal(elems[k]);
      }
    } catch (_) {
      for (var m = 0; m < elems.length; m++) reveal(elems[m]);
    }
  }

  // Debounce de scans para mutaciones agrupadas (SPAs).
  var scanScheduled = false;
  function scheduleScan() {
    if (scanScheduled) return;
    scanScheduled = true;
    setTimeout(function () {
      scanScheduled = false;
      scanRoot(document.body || document.documentElement);
    }, 50);
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

    scanRoot(document.body);

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
