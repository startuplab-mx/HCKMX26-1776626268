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
  return (
    <header
      style={{
        height: "52px",
        background: T.bg1,
        borderBottom: `1px solid ${T.border}`,
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "0 20px",
        flexShrink: 0,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
        <span
          style={{
            fontSize: "13px",
            fontWeight: 600,
            color: T.text0,
            fontFamily: "Space Grotesk",
          }}
        >
          Sentinel
        </span>
        <span style={{ color: T.border }}>·</span>
        <span style={{ fontSize: "12px", color: T.text1 }}>{labels[page]}</span>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: "16px" }}>
        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
          <PulseDot color={T.green} />
          <span
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
        <div style={{ width: "1px", height: "16px", background: T.border }} />
        <span
          style={{ fontSize: "11px", color: T.text2, fontFamily: "Space Grotesk" }}
        >
          {time.toLocaleTimeString("es-MX", { hour12: false })} UTC-6
        </span>
        <div
          style={{
            fontSize: "11px",
            color: T.text2,
            background: T.secondaryDim,
            padding: "3px 8px",
            borderRadius: "4px",
            border: `1px solid rgba(255,77,77,0.2)`,
          }}
        >
          <span style={{ color: T.secondary, fontWeight: 600 }}>{criticalCount} </span>
          alertas críticas
        </div>
      </div>
    </header>
  );
}
