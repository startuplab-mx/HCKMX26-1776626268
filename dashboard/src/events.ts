// Hook + tipos del feed de eventos de filtrado. Hace polling al backend
// Tauri (que internamente habla con el server Actix con el bearer token
// que `common::auth_token()` lee del entorno). El token NO se expone al
// webview.

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface Coords {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type FilterKind = "text" | "image";
export type FilterAction = "allow" | "warn" | "block";

export interface FilterEvent {
  id: string;
  kind: FilterKind;
  action: FilterAction;
  original: string;
  filtered: string;
  categories: string[];
  coords: Coords;
  url: string;
  timestamp_ms: number;
}

export function useFilterEvents(intervalMs: number = 2000) {
  const [events, setEvents] = useState<FilterEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const sinceRef = useRef<number>(0);

  useEffect(() => {
    let alive = true;

    async function tick() {
      try {
        const next = await invoke<FilterEvent[]>("fetch_events", {
          since: sinceRef.current || null,
        });
        if (!alive) return;
        if (next.length > 0) {
          // Append y mantén orden cronológico ascendente.
          setEvents((prev) => [...prev, ...next]);
          sinceRef.current = Math.max(
            sinceRef.current,
            ...next.map((e) => e.timestamp_ms),
          );
        }
        setError(null);
      } catch (e) {
        if (alive) setError(String(e));
      }
    }

    tick();
    const id = setInterval(tick, intervalMs);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [intervalMs]);

  async function clear() {
    await invoke("clear_events");
    setEvents([]);
    sinceRef.current = 0;
  }

  return { events, error, clear };
}
