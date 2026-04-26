// Content filter inyectado en cada navegación de la WebView de "browser_pane".
// Corre antes que cualquier script de la página gracias a `initialization_script`
// (desktop) o `WKUserScript`/`evaluateJavascript` at-document-start (mobile).
//
// v3 — selective blur:
//   - Imagen: el JS hace fetch (cache hit del browser) y manda los bytes raw
//     vía `invoke('filter_image_bytes', { bytes: Uint8Array })`. Rust corre
//     MobileCLIP-S1 (zero-shot) y devuelve los bytes ORIGINALES si la imagen
//     es benigna o bytes JPEG borrosos si entra en una categoría de riesgo.
//     Cero base64, cero double-download. En mobile (sin __TAURI_INTERNALS__)
//     cae a la API URL legacy con blur incondicional.
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
    // Selectores compuestos: el bloque queda transparente sólo mientras
    // NO esté ya filtrado (`__sb_done`) ni en estado skeleton (`__sb_skel`).
    // Cuando un text node se manda al clasificador, su ancestro p/h gana
    // `__sb_skel` y deja de aplicarle la regla `transparent` para que el
    // usuario vea los `-` que sustituyen al texto pendiente.
    pre.textContent =
      "p:not(.__sb_done):not(.__sb_skel),h1:not(.__sb_done):not(.__sb_skel)," +
      "h2:not(.__sb_done):not(.__sb_skel),h3:not(.__sb_done):not(.__sb_skel)," +
      "h4:not(.__sb_done):not(.__sb_skel),h5:not(.__sb_done):not(.__sb_skel)," +
      "h6:not(.__sb_done):not(.__sb_skel){color:transparent !important;text-shadow:none !important;}" +
      "p.__sb_skel,h1.__sb_skel,h2.__sb_skel,h3.__sb_skel,h4.__sb_skel," +
      "h5.__sb_skel,h6.__sb_skel{color:rgba(120,120,120,0.55) !important;text-shadow:none !important;}" +
      "img:not(.__sb_done){filter:blur(24px) !important;}" +
      // Spinner CSS-only via `html::before`. El pseudo-element se renderiza
      // desde el primer paint (no depende de que <body> exista ni de que
      // documentElement acepte children no estándar — WebKit a veces los
      // ignora). Ningún JS del sitio puede borrarlo. Centrado con
      // inset:0+margin:auto (el `transform` lo reservamos para la rotation
      // de la animación). Se oculta añadiendo `.__sb_loader_done` al <html>;
      // un `<style>` extra lo aniquila tras la transición para liberar la
      // animation y evitar repaint continuo.
      "html::before{content:'';position:fixed !important;" +
      "top:0 !important;left:0 !important;right:0 !important;bottom:0 !important;" +
      "margin:auto !important;width:32px !important;height:32px !important;" +
      "border:3px solid rgba(0,0,0,0.18) !important;border-top-color:#fff !important;" +
      "border-radius:50% !important;background:transparent !important;" +
      "box-sizing:border-box !important;display:block !important;" +
      "z-index:2147483647 !important;pointer-events:none !important;" +
      "filter:drop-shadow(0 0 6px rgba(0,0,0,0.55)) !important;" +
      "animation:__sb_spin 0.9s linear infinite !important;" +
      "transition:opacity 250ms ease !important;}" +
      "html.__sb_loader_done::before{opacity:0 !important;" +
      "animation-play-state:paused !important;}" +
      "@keyframes __sb_spin{to{transform:rotate(360deg)}}";
    (document.head || document.documentElement).appendChild(pre);
  } catch (_) {}

  // (1.5) Loader: ya está dibujado por `html::before` del CSS de arriba.
  // No insertamos ningún elemento DOM — ningún script del sitio puede
  // borrarlo, y aparece desde el primer paint sin depender del orden de
  // construcción del body.

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

  // ---------- 2.5 Loader overlay + skeleton text ----------
  // El loader cubre toda la WebView mientras se procesa el primer chunk de
  // textos. Se quita en el primer batch resuelto (éxito o error). Mientras
  // tanto, los nodos pendientes muestran sus letras/dígitos sustituidos por
  // `-` para que jamás se exponga texto sin clasificar.
  var loaderHidden = false;

  // CSS-only loader: no hay nada que asegurar — el `html::before` del
  // pre-hide CSS ya está dibujando el spinner. Se mantiene la función
  // como no-op para no tocar los call-sites de `runScan`/`onReady`.
  function ensureLoader() {}

  function hideLoader() {
    if (loaderHidden) return;
    loaderHidden = true;
    try {
      document.documentElement.classList.add("__sb_loader_done");
    } catch (_) {}
    // Tras la fade transition, aniquilamos el pseudo-element con un style
    // override (display:none + content:none + animation:none). Eso libera
    // la animation y evita repaint continuo.
    setTimeout(function () {
      try {
        var kill = document.createElement("style");
        kill.id = "__sb_loader_kill";
        kill.textContent =
          "html::before{display:none !important;content:none !important;animation:none !important;}";
        (document.head || document.documentElement).appendChild(kill);
      } catch (_) {}
    }, 350);
  }

  // Reemplaza letras (\p{L}) y dígitos (\p{N}) por '-'. Espacios y puntuación
  // se preservan para mantener la silueta visual del texto pendiente.
  var SKEL_RE;
  try { SKEL_RE = new RegExp("[\\p{L}\\p{N}]", "gu"); }
  catch (_) { SKEL_RE = /[A-Za-z0-9À-ɏ]/g; }
  function skeletonize(s) {
    try { return s.replace(SKEL_RE, "-"); }
    catch (_) { return s; }
  }

  // Sube al primer ancestor p/h1-h6 (los mismos que cubre el pre-hide CSS) y
  // le añade `__sb_skel`. Eso libera la regla `transparent` y permite que el
  // skeleton se vea en gris dim.
  function markSkel(node) {
    var p = node.parentNode;
    while (p && p.nodeType === 1) {
      var t = p.tagName;
      if (t === "P" || (t && t.length === 2 && t.charAt(0) === "H" &&
          t.charAt(1) >= "1" && t.charAt(1) <= "6")) {
        try { p.classList.add("__sb_skel"); } catch (_) {}
        return;
      }
      if (t === "BODY") return;
      p = p.parentNode;
    }
  }

  // ---------- 3. Bridges multiplataforma ----------

  // TEXTO BATCHED — desktop: invoke nativo con items{text,coords}+pageUrl.
  // Mobile: bridge nativo (filterTexts) sigue con array de strings, sin
  // coordenadas, hasta que se implemente el reporte de eventos en mobile.
  // Timeout duro: en iOS el FFI procesa TODOS los textos en una sola
  // llamada (CHUNK_SIZE=12 dentro de Rust, pero cada session.run es
  // ~5s en device para mDeBERTa-base — 4 chunks → ~16-20s típico para
  // 40 textos). Antes era 7s y se disparaba SIEMPRE en iOS,
  // devolviendo passthrough (originales) y el filtrado real llegaba
  // después demasiado tarde para aplicarse. 60s cubre el peor caso.
  var FILTER_TIMEOUT_MS = 60000;
  function callFilterTexts(items, pageUrl) {
    var texts = items.map(function (it) { return it.text; });
    var underlying;
    try {
      if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke) {
        underlying = window.__TAURI_INTERNALS__.invoke("filter_texts", {
          items: items,
          pageUrl: pageUrl,
        });
      } else if (
        window.webkit &&
        window.webkit.messageHandlers &&
        window.webkit.messageHandlers.filterTexts
      ) {
        // iOS WKScriptMessageHandlerWithReply — el handler nativo procesa el
        // array completo y devuelve [String].
        underlying = window.webkit.messageHandlers.filterTexts.postMessage(texts);
      } else if (window.FilterBridge && window.FilterBridge.filterTexts) {
        underlying = new Promise(function (resolve) {
          var id = "ts" + Date.now() + "_" + Math.random().toString(36).slice(2);
          (window.__filterCb = window.__filterCb || {})[id] = resolve;
          window.FilterBridge.filterTexts(id, JSON.stringify(texts));
        });
      } else {
        return Promise.resolve(texts.slice());
      }
    } catch (_) {
      return Promise.resolve(texts.slice());
    }
    return Promise.race([
      underlying,
      new Promise(function (resolve) {
        setTimeout(function () { resolve(texts.slice()); }, FILTER_TIMEOUT_MS);
      }),
    ]);
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

  // ---------- 4.5 Helpers de coordenadas (CSS px relativos al viewport) ----------
  // El dashboard usa esto para ubicar visualmente los eventos de filtrado.
  function rectToCoords(r) {
    return {
      x: Math.round(r.left || 0),
      y: Math.round(r.top || 0),
      width: Math.round(r.width || 0),
      height: Math.round(r.height || 0),
    };
  }
  function elementCoords(el) {
    try {
      if (!el || !el.getBoundingClientRect) return { x: 0, y: 0, width: 0, height: 0 };
      return rectToCoords(el.getBoundingClientRect());
    } catch (_) {
      return { x: 0, y: 0, width: 0, height: 0 };
    }
  }
  function textNodeCoords(node) {
    // Range cubre el text node real — más preciso que el bounding del padre
    // (que puede contener varios hermanos).
    try {
      var rng = document.createRange();
      rng.selectNodeContents(node);
      var r = rng.getBoundingClientRect();
      rng.detach && rng.detach();
      if (r && (r.width || r.height)) return rectToCoords(r);
    } catch (_) {}
    return elementCoords(node.parentElement);
  }

  // ---------- 5. Procesamiento de imagen ----------
  // reveal() también limpia `__sb_skel` por si algún elemento llega aquí
  // sin pasar por revealAncestorBlock (p.ej. el barrido final del scan que
  // levanta p/h sin candidatos textuales).
  function reveal(el) {
    try { el.classList.remove("__sb_skel"); } catch (_) {}
    el.classList.add("__sb_done");
  }

  // Path desktop: fetch del browser (cache hit) → bytes raw → invoke binario →
  // bytes blurred → blob URL. Manda coords + URL para reportar el evento al
  // server / dashboard.
  async function processImgDesktop(img, src) {
    if (imageCache[src]) {
      var cached = await imageCache[src];
      if (cached) {
        try { img.src = cached; } catch (_) {}
        reveal(img);
      }
      return;
    }
    // Snap coords antes del await — la imagen puede haberse movido cuando
    // el invoke regrese.
    var coords = elementCoords(img);
    var pageUrl = location.href || "";

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
        { bytes: u8, coords: coords, pageUrl: pageUrl, imageUrl: src }
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
      // Página sin texto sustancial (about:blank, login pages, etc.): no hay
      // razón para seguir mostrando el loader.
      hideLoader();
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
          // Quitar el estado skeleton además de marcar `__sb_done` — sin
          // esto los `-` quedarían en gris dim aunque ya estén reemplazados
          // por el resultado del clasificador.
          try { p.classList.remove("__sb_skel"); } catch (_) {}
          reveal(p);
          return;
        }
        if (tag === "BODY") return;
        p = p.parentNode;
      }
    }

    // Skeletonizar TODOS los candidatos antes de mandarlos al backend.
    // El usuario ve la silueta del texto en `-` desde ahora — jamás el
    // texto original sin pasar por el clasificador. El nodeValue real
    // queda guardado en `__sb_orig` (propiedad expando del Text node, GC
    // safe) para poder restaurar en error o al asignar el resultado.
    for (var sk = 0; sk < candidates.length; sk++) {
      try {
        candidates[sk].__sb_orig = candidates[sk].nodeValue;
        candidates[sk].nodeValue = skeletonize(candidates[sk].nodeValue);
        markSkel(candidates[sk]);
      } catch (_) {}
    }

    var firstChunkResolved = false;
    for (var start = 0; start < candidates.length; start += STREAM_CHUNK) {
      var slice = candidates.slice(start, start + STREAM_CHUNK);
      var sliceTexts = [];
      var sliceItems = [];
      for (var c = 0; c < slice.length; c++) {
        // El nodeValue actual ya es skeleton; mandamos el original
        // anotado en __sb_orig para que el clasificador vea texto real.
        var orig = slice[c].__sb_orig != null ? slice[c].__sb_orig : slice[c].nodeValue;
        sliceTexts.push(orig);
        sliceItems.push({
          text: orig,
          coords: textNodeCoords(slice[c]),
        });
      }

      try {
        var out = await callFilterTexts(sliceItems, location.href || "");
        var changed = 0;
        for (var k = 0; k < slice.length; k++) {
          var v = (out && out[k] != null) ? out[k] : sliceTexts[k];
          if (v !== sliceTexts[k]) changed++;
          try { slice[k].nodeValue = v; } catch (_) {}
          processedTextNodes.add(slice[k]);
          // Revelar el bloque padre apenas tenemos su texto filtrado, en vez
          // de esperar al final del scan completo. revealAncestorBlock
          // también limpia `__sb_skel`.
          revealAncestorBlock(slice[k]);
        }
        // Diag: si "0 / N filtered" en cada chunk durante mucho tiempo, lo
        // más probable es que el clasificador no esté cargado (revisar log
        // del backend: "[classifier] desactivado: ..."). Si filtra >0, está
        // funcionando aunque visualmente el resultado de AVISAR (letras→`-`)
        // se vea idéntico al skeleton.
        try {
          console.log("[shield-filter] chunk " +
            (Math.floor(start / STREAM_CHUNK) + 1) + ": " +
            changed + " / " + slice.length + " filtered");
        } catch (_) {}
        if (!firstChunkResolved) {
          firstChunkResolved = true;
          hideLoader();
        }
      } catch (_) {
        // Restaurar el texto original — el clasificador falló pero seguimos
        // política conservadora (mejor el texto bruto que dejar bloques en
        // `-` permanente y un loader infinito). El reveal limpia __sb_skel.
        for (var m = 0; m < slice.length; m++) {
          try { slice[m].nodeValue = sliceTexts[m]; } catch (_) {}
          processedTextNodes.add(slice[m]);
          revealAncestorBlock(slice[m]);
        }
        hideLoader();
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
    // Reasegurar el loader antes de cada scan: si un MutationObserver del
    // sitio destruyó el div o si el primer scan corrió antes de tener body,
    // este es nuestro segundo intento. Es no-op si ya está visible o ya
    // se ocultó (loaderHidden=true es sticky).
    ensureLoader();
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

    // Mostrar el loader lo antes posible — antes de agendar el primer scan,
    // antes incluso de que el clasificador haga su primer roundtrip. Aquí ya
    // existe document.body por DOMContentLoaded.
    ensureLoader();
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
