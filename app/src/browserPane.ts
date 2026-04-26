import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// El plugin nativo expone UN solo WKWebView/WebView. Para preservar estado
// (login, scroll, navegación interna) entre montajes de las páginas que lo
// usan, rastreamos quién es su "dueño" actual. Si una página se vuelve a
// montar y sigue siendo la dueña, sólo re-sincronizamos los bounds; si la
// dueña cambió, navegamos a la URL nueva (lo que descarta el estado previo).

export type BrowserOwner = "navegador" | "facebook" | "instagram";

let currentOwner: BrowserOwner | null = null;

export function getBrowserOwner(): BrowserOwner | null {
  return currentOwner;
}

export function setBrowserOwner(owner: BrowserOwner | null): void {
  currentOwner = owner;
}

/**
 * Hook para páginas con URL fija (Facebook, Instagram). Maneja:
 *  - Sincronización de bounds del WebView nativo con un placeholder en React.
 *  - Apertura inicial: reusa el WebView si esta página ya era la dueña
 *    (preserva estado), o abre/navega a la URL fija si no lo era.
 *  - Eventos browser-navigated / browser-blocked.
 *  - Al desmontar: oculta el WebView (bounds 0×0) en lugar de cerrarlo, para
 *    que el estado sobreviva al regreso al home.
 */
export function useEmbeddedBrowser(opts: { owner: BrowserOwner; url: string }) {
  const { owner, url } = opts;
  const paneRef = useRef<HTMLDivElement | null>(null);
  const [navError, setNavError] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  const ownedRef = useRef(false);

  const updateBounds = useCallback(async () => {
    if (!paneRef.current) return;
    const r = paneRef.current.getBoundingClientRect();
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

  // Apertura inicial: si la dueña actual somos nosotros, sólo re-sincronizamos
  // bounds; si no, abrimos/navegamos a la URL fija (y nos volvemos la dueña).
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const reuse = getBrowserOwner() === owner;
      try {
        if (reuse) {
          // El WebView nativo sigue vivo y mostrando nuestro contenido;
          // sólo lo movemos a la posición visible.
          ownedRef.current = true;
          await updateBounds();
        } else {
          // Otra página dejó el WebView vivo (oculto a 0×0). Reusarlo haría
          // que muestre brevemente su contenido viejo al expandirlo a
          // nuestros bounds antes de que cargue nuestra URL. Ciérralo para
          // que el `open` cree uno fresco con fondo blanco.
          if (getBrowserOwner() !== null) {
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
          if (cancelled) return;
          ownedRef.current = true;
          setBrowserOwner(owner);
        }
        if (!cancelled) {
          setReady(true);
          // Re-sync por si el WebView se reajustó tras abrir/navegar.
          requestAnimationFrame(() => void updateBounds());
          setTimeout(() => void updateBounds(), 100);
          setTimeout(() => void updateBounds(), 400);
        }
      } catch (err) {
        if (!cancelled) setNavError(humanizeError(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [owner, url, updateBounds]);

  // Listeners para navegación interna del WebView (clicks dentro del sitio).
  useEffect(() => {
    let unlistenNav: UnlistenFn | null = null;
    let unlistenBlock: UnlistenFn | null = null;
    listen<string>("browser-navigated", () => {
      setNavError(null);
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

  // Mantén los bounds sincronizados cuando cambia el tamaño de la ventana o
  // del placeholder (e.g. teclado, rotación).
  useEffect(() => {
    if (!ready) return;
    const ro = new ResizeObserver(() => void updateBounds());
    if (paneRef.current) ro.observe(paneRef.current);
    const onResize = () => void updateBounds();
    window.addEventListener("resize", onResize);
    void updateBounds();
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", onResize);
    };
  }, [ready, updateBounds]);

  // Al desmontar: ocultar el WebView (bounds 0×0) en lugar de cerrarlo,
  // para que conserve estado para el próximo montaje.
  useEffect(() => {
    return () => {
      if (!ownedRef.current) return;
      invoke("set_browser_view_bounds", {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
      }).catch(() => {});
    };
  }, []);

  return { paneRef, navError, ready };
}

function humanizeError(err: unknown): string {
  const raw = String(err);
  if (raw.includes("not open")) return "El navegador interno no está disponible.";
  if (raw.includes("invalid URL")) return "URL inválida.";
  if (raw.length > 160) return "No se pudo abrir la página.";
  return raw;
}
