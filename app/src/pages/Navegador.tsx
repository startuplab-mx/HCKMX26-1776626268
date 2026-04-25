import { useCallback, useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, Loader2, Search } from "lucide-react";
import AppShell from "../components/AppShell";

type PageState = {
  url: string;
  title: string;
  html: string;
};

type SandboxMessage =
  | { source: "sandbox-browser"; kind: "navigate"; url: string }
  | {
      source: "sandbox-browser";
      kind: "click" | "input" | "change" | "submit" | "key";
      selector: string;
      value?: string;
    };

export default function Navegador() {
  const [inputUrl, setInputUrl] = useState("");
  const [page, setPage] = useState<PageState | null>(null);
  const [loading, setLoading] = useState(false);
  const [navError, setNavError] = useState<string | null>(null);
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const eventInFlight = useRef(false);
  const pageRef = useRef<PageState | null>(null);

  const applyPage = useCallback((next: PageState) => {
    pageRef.current = next;
    setPage(next);
    setInputUrl(next.url);
  }, []);

  const navigate = useCallback(
    async (raw: string) => {
      const url = normalizeOrSearch(raw);
      if (!url) return;
      setLoading(true);
      setNavError(null);
      try {
        const next = await invoke<PageState>("browser_navigate", { url });
        applyPage(next);
      } catch (err) {
        setNavError(humanizeError(err));
      } finally {
        setLoading(false);
      }
    },
    [applyPage],
  );

  const sendEvent = useCallback(
    async (
      kind: "click" | "input" | "change" | "submit" | "key",
      selector: string,
      value?: string,
    ) => {
      // input/change: fire-and-forget. Mantenemos el iframe estable mientras el
      // usuario tipea. Solo replicamos el valor en Camoufox.
      if (kind === "input" || kind === "change") {
        invoke<PageState>("browser_event", {
          kind,
          selector,
          value: value ?? null,
        }).catch((err) => console.warn("[navegador] input ignored:", err));
        return;
      }
      if (eventInFlight.current) return;
      eventInFlight.current = true;
      setLoading(true);
      try {
        const next = await invoke<PageState>("browser_event", {
          kind,
          selector,
          value: value ?? null,
        });
        // Solo re-renderizar el iframe si la URL cambió (navegación real).
        // Si fue un click que no navegó (ej. enfocar un input), conservamos el
        // srcDoc actual para que el iframe NO se re-monte y el foco persista.
        const prevUrl = pageRef.current?.url;
        if (!prevUrl || next.url !== prevUrl) {
          applyPage(next);
        }
      } catch (err) {
        console.warn("[navegador] event ignored:", err);
      } finally {
        eventInFlight.current = false;
        setLoading(false);
      }
    },
    [applyPage],
  );

  useEffect(() => {
    function onMessage(event: MessageEvent) {
      const data = event.data as SandboxMessage | undefined;
      if (!data || data.source !== "sandbox-browser") return;
      if (data.kind === "navigate") {
        void navigate(data.url);
        return;
      }
      void sendEvent(data.kind, data.selector, data.value);
    }
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [navigate, sendEvent]);

  return (
    <AppShell>
      <div className="flex items-center gap-2 mb-3">
        <Link
          to="/"
          aria-label="Volver"
          className="px-3 py-2 rounded-lg bg-white/10 hover:bg-white/20 transition-colors flex items-center justify-center"
        >
          <ArrowLeft className="w-4 h-4" strokeWidth={2} />
        </Link>
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

      <div className="flex-1 rounded-xl overflow-hidden bg-white/10 backdrop-blur-sm">
        <iframe
          ref={iframeRef}
          sandbox="allow-scripts"
          srcDoc={page?.html ?? EMPTY_DOC}
          title="Sandbox browser"
          className="w-full h-full bg-transparent border-0"
        />
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
  // DuckDuckGo HTML: server-renderizado, sin JS, sin anti-bot, sin tracking.
  // Google fue rechazado porque /sorry y reCAPTCHA bloquean automation.
  return `https://html.duckduckgo.com/html/?q=${encodeURIComponent(t)}`;
}

function humanizeError(err: unknown): string {
  const raw = String(err);
  if (raw.includes("Timeout")) return "La página tardó demasiado en responder.";
  if (raw.includes("net::") || raw.includes("connect"))
    return "No se pudo conectar al sitio.";
  if (raw.includes("Bad Gateway") || raw.includes("502"))
    return "El navegador interno no respondió.";
  if (raw.length > 160) return "No se pudo cargar la página.";
  return raw;
}

const EMPTY_DOC = `<!doctype html><html><head><meta charset="utf-8"><style>
  html, body { margin:0; padding:0; height:100%; background:transparent; }
  body {
    color: rgba(255,255,255,0.65);
    font-family: -apple-system, BlinkMacSystemFont, "Inter", system-ui, sans-serif;
    font-size: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
</style></head><body>Busca algo o escribe una URL para empezar.</body></html>`;
