import { useEffect, useState } from "react";
import { T } from "../theme";
import type { PageId } from "../types";
import { PulseDot } from "./PulseDot";

const labels: Record<PageId, string> = {
  overview: "Service Overview",
  incidents: "Gestión de Eventos",
  settings: "Configuración",
};

interface Props {
  page: PageId;
  criticalCount?: number;
}

export function Header({ page, criticalCount = 0 }: Props) {
  const [time, setTime] = useState(new Date());
  useEffect(() => {
    const t = setInterval(() => setTime(new Date()), 1000);
    return () => clearInterval(t);
  }, []);
  // Tauri v2 chequea `data-tauri-drag-region` en el target exacto del click,
  // NO recorre el DOM hacia arriba. Por eso los hijos no heredan el drag —
  // hay que poner el atributo explícitamente en cada hijo (divs y spans).
  // Si todos los descendientes lo tienen, cualquier click dentro del header
  // mueve la ventana, igual que un title bar nativo.
  const drag = { "data-tauri-drag-region": "" } as Record<string, string>;
  return (
    <header
      {...drag}
      style={{
        height: "52px",
        background: T.bg0,
        borderBottom: `1px solid ${T.border}`,
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        // Padding izquierdo extra en macOS para no chocar con los
        // traffic lights del title bar overlay (close/min/max).
        padding: "0 20px 0 84px",
        flexShrink: 0,
      }}
    >
      <div {...drag} style={{ display: "flex", alignItems: "center", gap: "12px" }}>
        <span
          {...drag}
          style={{
            fontSize: "13px",
            fontWeight: 600,
            color: T.text0,
            fontFamily: "Space Grotesk",
          }}
        >
          Sentinel
        </span>
        <span {...drag} style={{ color: T.border }}>·</span>
        <span {...drag} style={{ fontSize: "12px", color: T.text1 }}>
          {labels[page]}
        </span>
      </div>
      <div {...drag} style={{ display: "flex", alignItems: "center", gap: "16px" }}>
        <div {...drag} style={{ display: "flex", alignItems: "center", gap: "6px" }}>
          <PulseDot color={T.green} />
          <span
            {...drag}
            style={{
              fontSize: "11px",
              color: T.green,
              fontFamily: "Space Grotesk",
              fontWeight: 500,
            }}
          >
            SISTEMA ACTIVO
          </span>
        </div>
        <div {...drag} style={{ width: "1px", height: "16px", background: T.border }} />
        <span
          {...drag}
          style={{ fontSize: "11px", color: T.text2, fontFamily: "Space Grotesk" }}
        >
          {time.toLocaleTimeString("es-MX", { hour12: false })} UTC-6
        </span>
        <div
          {...drag}
          style={{
            fontSize: "11px",
            color: T.text2,
            background: T.secondaryDim,
            padding: "3px 8px",
            borderRadius: "4px",
            border: `1px solid rgba(255,77,77,0.2)`,
          }}
        >
          <span {...drag} style={{ color: T.secondary, fontWeight: 600 }}>
            {criticalCount}{" "}
          </span>
          alertas críticas
        </div>
      </div>
    </header>
  );
}
