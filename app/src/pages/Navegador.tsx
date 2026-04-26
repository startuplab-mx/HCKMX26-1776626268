import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ArrowLeft, Loader2, Search } from "lucide-react";
import AppShell from "../components/AppShell";
import { getBrowserOwner, setBrowserOwner } from "../browserPane";

export default function Navegador() {
  const [inputUrl, setInputUrl] = useState("");
  const [loading, setLoading] = useState(false);
  const [navError, setNavError] = useState<string | null>(null);
  const [hasView, setHasView] = useState(false);
  const paneRef = useRef<HTMLDivElement | null>(null);
  const hasViewRef = useRef(false);
  const routerNavigate = useNavigate();

  const dismissAndGoHome = useCallback(async () => {
    // Cerrar el WebView nativo ANTES de navegar fuera, para evitar que el
    // unmount de React lo deje colgando en pantalla.
    try {
      await invoke("close_browser_view");
    } catch (_) {
      // ignore
    }
    hasViewRef.current = false;
    if (getBrowserOwner() === "navegador") setBrowserOwner(null);
    routerNavigate("/");
  }, [routerNavigate]);

  const updateBounds = useCallback(async () => {
    if (!paneRef.current || !hasViewRef.current) return;
    const r = paneRef.current.getBoundingClientRect();
    console.log("[browser_pane] paneRef bounds", {
      x: r.left,
      y: r.top,
      w: r.width,
      h: r.height,
      windowInner: { w: window.innerWidth, h: window.innerHeight },
      dpr: window.devicePixelRatio,
    });
    try {
      await invoke("set_browser_view_bounds", {
        x: r.left,
        y: r.top,
        width: r.width,
        height: r.height,
      });
    } catch (e) {
      console.warn("set_browser_view_bounds failed:", e);
    }
  }, []);

  useEffect(() => {
    let unlistenNav: UnlistenFn | null = null;
    let unlistenBlock: UnlistenFn | null = null;
    listen<string>("browser-navigated", (e) => {
      setInputUrl(e.payload);
      setNavError(null);
      // El webview puede haber reajustado su frame al navegar — re-sync bounds.
      requestAnimationFrame(() => void updateBounds());
      setTimeout(() => void updateBounds(), 200);
    }).then((u) => {
      unlistenNav = u;
    });
    listen<string>("browser-blocked", (e) => {
      setNavError(`Sitio bloqueado: ${e.payload}`);
    }).then((u) => {
      unlistenBlock = u;
    });
    return () => {
      unlistenNav?.();
      unlistenBlock?.();
    };
  }, [updateBounds]);

  useEffect(() => {
    if (!hasView) return;
    const ro = new ResizeObserver(() => void updateBounds());
    if (paneRef.current) ro.observe(paneRef.current);
    const onResize = () => void updateBounds();
    window.addEventListener("resize", onResize);
    void updateBounds();
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", onResize);
    };
  }, [hasView, updateBounds]);

  useEffect(() => {
    return () => {
      invoke("close_browser_view").catch(() => {});
      if (getBrowserOwner() === "navegador") setBrowserOwner(null);
    };
  }, []);

  const navigate = useCallback(
    async (raw: string) => {
      const url = normalizeOrSearch(raw);
      if (!url) return;
      setLoading(true);
      setNavError(null);
      try {
        if (!hasViewRef.current) {
          // Si otra página (Facebook / Instagram) dejó su WebView oculto,
          // ciérralo antes para que su contenido no aparezca brevemente
          // en nuestros bounds mientras carga la URL solicitada.
          const prev = getBrowserOwner();
          if (prev !== null && prev !== "navegador") {
            try {
              await invoke("close_browser_view");
            } catch (_) {
              // ignore
            }
            setBrowserOwner(null);
          }
          const r = paneRef.current?.getBoundingClientRect();
          await invoke("open_browser_view", {
            url,
            x: r?.left ?? 0,
            y: r?.top ?? 80,
            width: r?.width ?? 800,
            height: r?.height ?? 600,
          });
          hasViewRef.current = true;
          setHasView(true);
          setBrowserOwner("navegador");
        } else {
          await invoke("navigate_browser_view", { url });
          setBrowserOwner("navegador");
        }
        setInputUrl(url);
        // Re-sincronizar bounds tras la creación / navegación. El webview
        // a veces se reajusta a un tamaño default antes de honrar el nuestro.
        requestAnimationFrame(() => void updateBounds());
        setTimeout(() => void updateBounds(), 100);
        setTimeout(() => void updateBounds(), 400);
      } catch (err) {
        setNavError(humanizeError(err));
      } finally {
        setLoading(false);
      }
    },
    [updateBounds],
  );

  return (
    <AppShell className="gap-3" iosTopExtra="0px">
      <div className="flex items-center gap-2 mt-4">
        <button
          type="button"
          aria-label="Volver"
          onClick={() => void dismissAndGoHome()}
          className="px-3 py-2 rounded-lg bg-white/10 hover:bg-white/20 transition-colors flex items-center justify-center"
        >
          <ArrowLeft className="w-4 h-4" strokeWidth={2} />
        </button>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void navigate(inputUrl);
          }}
          className="flex-1 flex items-center gap-2"
        >
          <input
            type="text"
            value={inputUrl}
            onChange={(e) => setInputUrl(e.target.value)}
            onFocus={(e) => e.currentTarget.select()}
            placeholder="Busca o escribe una URL"
            className="flex-1 px-3 py-2 rounded-lg bg-white/10 placeholder-white/50 text-white outline-none focus:bg-white/15 focus:ring-1 focus:ring-white/30 transition"
          />
          <button
            type="submit"
            aria-label="Buscar"
            disabled={loading}
            className="px-3 py-2 rounded-lg bg-white/10 hover:bg-white/20 disabled:opacity-60 transition-colors flex items-center justify-center"
          >
            {loading ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Search className="w-4 h-4" strokeWidth={2} />
            )}
          </button>
        </form>
      </div>

      {navError && (
        <div className="mb-3 px-3 py-2 rounded-lg bg-red-500/20 text-red-100 text-sm">
          {navError}
        </div>
      )}

      <div
        ref={paneRef}
        className="flex-1 rounded-xl bg-white/5 backdrop-blur-sm flex items-center justify-center text-white/60 text-sm"
      >
        {!hasView && <span>Busca o escribe una URL para empezar.</span>}
      </div>
    </AppShell>
  );
}

function normalizeOrSearch(raw: string): string {
  const t = raw.trim();
  if (!t) return "";
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(t)) return t;
  if (!/\s/.test(t) && /^[^\s]+\.[a-z]{2,}([\/?#].*)?$/i.test(t)) {
    return `https://${t}`;
  }
  if (!/\s/.test(t) && /^localhost(:\d+)?([\/?#].*)?$/i.test(t)) {
    return `https://${t}`;
  }
  return `https://html.duckduckgo.com/html/?q=${encodeURIComponent(t)}`;
}

function humanizeError(err: unknown): string {
  const raw = String(err);
  if (raw.includes("not open")) return "El navegador interno no está disponible.";
  if (raw.includes("invalid URL")) return "URL inválida.";
  if (raw.length > 160) return "No se pudo abrir la página.";
  return raw;
}
