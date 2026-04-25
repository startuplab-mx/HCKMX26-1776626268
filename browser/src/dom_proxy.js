(function () {
  if (window.__sandboxBrowserProxyInstalled) return;
  window.__sandboxBrowserProxyInstalled = true;

  var SOURCE = "sandbox-browser";
  var cfg = window.__SANDBOX || { proxy: "/asset", token: "" };
  var PROXY = cfg.proxy;
  var TOKEN = cfg.token;

  // ---- Storage / cookie / history shims ----
  // En un iframe con sandbox="allow-scripts" (sin allow-same-origin) las APIs
  // de almacenamiento lanzan SecurityError. Sitios como Facebook llaman
  // localStorage en su bootstrap y todo el módulo muere. Reemplazamos con
  // implementaciones in-memory para que el JS pueda continuar.
  function fakeStorage() {
    var data = Object.create(null);
    return {
      getItem: function (k) {
        var v = data[k];
        return v === undefined ? null : v;
      },
      setItem: function (k, v) {
        data[k] = String(v);
      },
      removeItem: function (k) {
        delete data[k];
      },
      clear: function () {
        for (var k in data) delete data[k];
      },
      key: function (i) {
        return Object.keys(data)[i] || null;
      },
      get length() {
        return Object.keys(data).length;
      },
    };
  }
  try {
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      value: fakeStorage(),
    });
  } catch (_) {}
  try {
    Object.defineProperty(window, "sessionStorage", {
      configurable: true,
      value: fakeStorage(),
    });
  } catch (_) {}

  // document.cookie también puede throw — lo reemplazamos por un store volátil.
  try {
    var cookieStore = "";
    Object.defineProperty(Document.prototype, "cookie", {
      configurable: true,
      get: function () {
        return cookieStore;
      },
      set: function (v) {
        // Acumula cookies estilo "key=value; key2=value2".
        if (typeof v !== "string") return;
        var sep = v.indexOf(";");
        var pair = sep === -1 ? v : v.slice(0, sep);
        if (cookieStore) cookieStore += "; ";
        cookieStore += pair;
      },
    });
  } catch (_) {}

  // history.replaceState / pushState lanzan SecurityError al cambiar URL desde
  // about:srcdoc a un origen real. Las envolvemos para que swallow.
  try {
    var origReplace = history.replaceState.bind(history);
    history.replaceState = function (state, title, url) {
      try {
        return origReplace(state, title, url);
      } catch (_) {}
    };
    var origPush = history.pushState.bind(history);
    history.pushState = function (state, title, url) {
      try {
        return origPush(state, title, url);
      } catch (_) {}
    };
  } catch (_) {}

  function isProxyUrl(u) {
    return typeof u === "string" && u.indexOf(PROXY + "?u=") !== -1;
  }

  function isOpaqueScheme(u) {
    return /^(data|blob|javascript|about|mailto|tel):/i.test(u);
  }

  function proxify(url) {
    if (!url) return url;
    var s = String(url);
    if (isProxyUrl(s) || isOpaqueScheme(s) || s.charAt(0) === "#") return s;
    try {
      var abs = new URL(s, document.baseURI).toString();
      if (isOpaqueScheme(abs)) return abs;
      return PROXY + "?u=" + encodeURIComponent(abs) + "&t=" + encodeURIComponent(TOKEN);
    } catch (_) {
      return s;
    }
  }

  function proxifyKeepFragment(url) {
    if (!url) return url;
    var s = String(url);
    if (isProxyUrl(s) || isOpaqueScheme(s) || s.charAt(0) === "#") return s;
    try {
      var abs = new URL(s, document.baseURI).toString();
      if (isOpaqueScheme(abs)) return abs;
      var hashIdx = abs.indexOf("#");
      var urlPart = hashIdx === -1 ? abs : abs.slice(0, hashIdx);
      var fragment = hashIdx === -1 ? "" : abs.slice(hashIdx);
      return PROXY + "?u=" + encodeURIComponent(urlPart) + "&t=" + encodeURIComponent(TOKEN) + fragment;
    } catch (_) {
      return s;
    }
  }

  // ---- fetch shim ----
  if (typeof window.fetch === "function") {
    var origFetch = window.fetch.bind(window);
    window.fetch = function (input, init) {
      try {
        if (typeof Request !== "undefined" && input instanceof Request) {
          input = new Request(proxify(input.url), input);
        } else {
          input = proxify(String(input));
        }
      } catch (_) {}
      return origFetch(input, init);
    };
  }

  // ---- XHR shim ----
  if (typeof XMLHttpRequest !== "undefined") {
    var origOpen = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function (method, url) {
      var args = Array.prototype.slice.call(arguments);
      try {
        args[1] = proxify(String(url));
      } catch (_) {}
      return origOpen.apply(this, args);
    };
  }

  // ---- WebSocket stub ----
  function StubWS(url) {
    try { console.warn("[sandbox] WebSocket blocked:", url); } catch (_) {}
    var ws = Object.create(EventTarget.prototype);
    ws.url = String(url || "");
    ws.readyState = 3;
    ws.protocol = "";
    ws.bufferedAmount = 0;
    ws.binaryType = "blob";
    ws.send = function () {};
    ws.close = function () {};
    return ws;
  }
  StubWS.CONNECTING = 0;
  StubWS.OPEN = 1;
  StubWS.CLOSING = 2;
  StubWS.CLOSED = 3;
  window.WebSocket = StubWS;

  // ---- EventSource stub ----
  if (window.EventSource) {
    var StubES = function (url) {
      try { console.warn("[sandbox] EventSource blocked:", url); } catch (_) {}
      var es = Object.create(EventTarget.prototype);
      es.url = String(url || "");
      es.readyState = 2;
      es.withCredentials = false;
      es.close = function () {};
      return es;
    };
    StubES.CONNECTING = 0;
    StubES.OPEN = 1;
    StubES.CLOSED = 2;
    window.EventSource = StubES;
  }

  // ---- Dynamic DOM rewriter ----
  function rewriteAttr(el, attr) {
    if (!el || !el.getAttribute) return;
    var v = el.getAttribute(attr);
    if (!v) return;
    var p = proxify(v);
    if (p !== v) {
      try { el.setAttribute(attr, p); } catch (_) {}
    }
  }

  function rewriteSrcset(el) {
    if (!el || !el.getAttribute) return;
    var v = el.getAttribute("srcset");
    if (!v) return;
    var parts = v.split(",");
    var out = [];
    for (var i = 0; i < parts.length; i++) {
      var t = parts[i].trim();
      if (!t) continue;
      var sp = t.search(/\s/);
      var u = sp === -1 ? t : t.slice(0, sp);
      var d = sp === -1 ? "" : " " + t.slice(sp + 1).trim();
      out.push(proxify(u) + d);
    }
    var nv = out.join(", ");
    if (nv !== v) {
      try { el.setAttribute("srcset", nv); } catch (_) {}
    }
  }

  var SRC_TAGS = { IMG: 1, SCRIPT: 1, IFRAME: 1, SOURCE: 1, VIDEO: 1, AUDIO: 1, TRACK: 1, EMBED: 1 };
  var HREF_TAGS = { LINK: 1 };

  function rewriteUseLike(el) {
    if (!el || !el.getAttribute || !el.setAttribute) return;
    for (var k = 0; k < 2; k++) {
      var attr = k === 0 ? "href" : "xlink:href";
      var v = el.getAttribute(attr);
      if (!v) continue;
      var p = proxifyKeepFragment(v);
      if (p !== v) {
        try { el.setAttribute(attr, p); } catch (_) {}
      }
    }
  }

  function rewriteEl(el) {
    if (!el || !el.tagName) return;
    var lt = String(el.tagName).toLowerCase();
    if (lt === "a") return;
    if (lt === "use" || lt === "image") {
      rewriteUseLike(el);
      return;
    }
    if (SRC_TAGS[el.tagName]) {
      rewriteAttr(el, "src");
      rewriteSrcset(el);
    }
    if (HREF_TAGS[el.tagName]) {
      rewriteAttr(el, "href");
    }
    if (el.tagName === "INPUT" && el.getAttribute && el.getAttribute("type") === "image") {
      rewriteAttr(el, "src");
    }
  }

  // Inyecta CSS al final de <head> que fuerza pointer-events e visibilidad
  // en inputs/buttons/etc. Algunas páginas (Bloks de FB, frameworks server-
  // driven) aplican `pointer-events: none` al input real y delegan en JS para
  // routear clicks. Como strippeamos el JS de la página, sin este override el
  // click cae en el wrapper padre y nunca llega al input.
  function injectInteractivityOverrides() {
    if (document.getElementById("__sandbox_overrides__")) return;
    var s = document.createElement("style");
    s.id = "__sandbox_overrides__";
    s.textContent =
      "input,textarea,select,button,a{pointer-events:auto !important}" +
      "input,textarea,select{visibility:visible !important;opacity:1 !important;user-select:text !important}" +
      "*[style*='pointer-events:none']>input,*[style*='pointer-events:none']>textarea{pointer-events:auto !important}";
    (document.head || document.documentElement).appendChild(s);
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", injectInteractivityOverrides);
  } else {
    injectInteractivityOverrides();
  }

  function startObserver() {
    if (typeof MutationObserver === "undefined") return;
    var obs = new MutationObserver(function (mutations) {
      for (var i = 0; i < mutations.length; i++) {
        var m = mutations[i];
        if (m.type === "childList") {
          for (var j = 0; j < m.addedNodes.length; j++) {
            var n = m.addedNodes[j];
            if (!n || n.nodeType !== 1) continue;
            rewriteEl(n);
            if (n.querySelectorAll) {
              var matches = n.querySelectorAll("[src],[href],[srcset],[xlink\\:href]");
              for (var k = 0; k < matches.length; k++) rewriteEl(matches[k]);
            }
          }
        } else if (m.type === "attributes" && m.target) {
          rewriteEl(m.target);
        }
      }
    });
    obs.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["src", "href", "srcset", "xlink:href"],
    });
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", startObserver);
  } else {
    startObserver();
  }

  // ---- DOM event proxy ----
  function send(payload) {
    try {
      window.parent.postMessage(Object.assign({ source: SOURCE }, payload), "*");
    } catch (_) {}
  }

  function selectorFor(el) {
    if (!el || el.nodeType !== 1) return "";
    if (el.id) return "#" + CSS.escape(el.id);
    var parts = [];
    var node = el;
    while (node && node.nodeType === 1 && node !== document.documentElement) {
      var tag = node.tagName.toLowerCase();
      var parent = node.parentNode;
      if (!parent) {
        parts.unshift(tag);
        break;
      }
      var siblings = Array.prototype.filter.call(parent.children, function (c) {
        return c.tagName === node.tagName;
      });
      var index = siblings.indexOf(node) + 1;
      parts.unshift(tag + ":nth-of-type(" + index + ")");
      node = parent;
      if (node && node.id) {
        parts.unshift("#" + CSS.escape(node.id));
        break;
      }
    }
    return parts.join(" > ");
  }

  function absolutize(href) {
    try {
      return new URL(href, document.baseURI).toString();
    } catch (_) {
      return href;
    }
  }

  document.addEventListener(
    "click",
    function (e) {
      if (!e.isTrusted) return;
      var target = e.target instanceof Element ? e.target : null;
      if (!target) return;
      var anchor = target.closest && target.closest("a");
      if (anchor && anchor.href) {
        e.preventDefault();
        e.stopPropagation();
        send({ kind: "navigate", url: absolutize(anchor.getAttribute("href") || anchor.href) });
        return;
      }
      send({ kind: "click", selector: selectorFor(target) });
    },
    true
  );

  var inputTimer = null;
  document.addEventListener(
    "input",
    function (e) {
      if (!e.isTrusted) return;
      var target = e.target;
      if (!target || !("value" in target)) return;
      var selector = selectorFor(target);
      var value = target.value;
      if (inputTimer) clearTimeout(inputTimer);
      inputTimer = setTimeout(function () {
        send({ kind: "input", selector: selector, value: value });
      }, 150);
    },
    true
  );

  document.addEventListener(
    "change",
    function (e) {
      if (!e.isTrusted) return;
      var target = e.target;
      if (!target) return;
      send({
        kind: "change",
        selector: selectorFor(target),
        value: "value" in target ? target.value : "",
      });
    },
    true
  );

  document.addEventListener(
    "submit",
    function (e) {
      if (!e.isTrusted) return;
      var form = e.target;
      if (!form) return;
      e.preventDefault();
      if (inputTimer) {
        clearTimeout(inputTimer);
        inputTimer = null;
      }
      send({ kind: "submit", selector: selectorFor(form) });
    },
    true
  );

  document.addEventListener(
    "keydown",
    function (e) {
      if (!e.isTrusted) return;
      if (e.key !== "Enter") return;
      var target = e.target;
      if (!target || target.tagName !== "INPUT") return;
      send({ kind: "key", selector: selectorFor(target), value: "Enter" });
    },
    true
  );
})();
