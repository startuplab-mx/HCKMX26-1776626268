import { useState } from "react";
import { T } from "../theme";
import { riskConfig, type RiskFilter, type StateRow } from "../types";
import { RiskBadge } from "./RiskBadge";
import { RiskFilterButtons } from "./RiskFilter";

type SortCol = keyof StateRow;

const COLUMNS: { key: SortCol; label: string }[] = [
  { key: "estado", label: "Región" },
  { key: "altitud", label: "Altitud (msnm)" },
  { key: "lat", label: "Latitud" },
  { key: "incidentes", label: "Eventos" },
  { key: "riesgo", label: "Severidad" },
];

interface Props {
  rows: StateRow[];
}

export function StatesTable({ rows }: Props) {
  const [sort, setSort] = useState<{ col: SortCol; dir: 1 | -1 }>({
    col: "incidentes",
    dir: -1,
  });
  const [filter, setFilter] = useState<RiskFilter>("TODOS");

  const sorted = [...rows]
    .filter((r) => filter === "TODOS" || r.riesgo === filter)
    .sort((a, b) => {
      const av = a[sort.col];
      const bv = b[sort.col];
      return sort.dir * (av > bv ? 1 : av < bv ? -1 : 0);
    });

  const toggleSort = (col: SortCol) =>
    setSort((s) => ({ col, dir: s.col === col ? ((-s.dir) as 1 | -1) : -1 }));

  const arrow = (col: SortCol) =>
    sort.col === col ? (sort.dir === -1 ? " ↓" : " ↑") : "";

  const maxInc = Math.max(...rows.map((r) => r.incidentes), 1);

  return (
    <div
      style={{
        background: T.bg2,
        border: `1px solid ${T.border}`,
        borderRadius: "12px",
        display: "flex",
        flexDirection: "column",
      }}
    >
      <div
        style={{
          padding: "14px 16px",
          borderBottom: `1px solid ${T.border}`,
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <div>
          <span style={{ fontSize: "13px", fontWeight: 600, color: T.text0 }}>
            Eventos por Región
          </span>
          <span style={{ fontSize: "11px", color: T.text2, marginLeft: "8px" }}>
            · México
          </span>
        </div>
        <RiskFilterButtons value={filter} onChange={setFilter} />
      </div>
      <div style={{ overflow: "auto" }}>
        <table style={{ width: "100%", borderCollapse: "collapse" }}>
          <thead>
            <tr style={{ borderBottom: `1px solid ${T.border}` }}>
              {COLUMNS.map((h) => (
                <th
                  key={h.key}
                  onClick={() => toggleSort(h.key)}
                  style={{
                    padding: "9px 14px",
                    textAlign: "left",
                    fontSize: "10px",
                    fontFamily: "Space Grotesk",
                    color: sort.col === h.key ? T.primary : T.text2,
                    fontWeight: 500,
                    letterSpacing: "0.07em",
                    textTransform: "uppercase",
                    cursor: "pointer",
                    userSelect: "none",
                    background: T.bg2,
                    whiteSpace: "nowrap",
                  }}
                >
                  {h.label}
                  {arrow(h.key)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {sorted.map((row) => {
              const rc = riskConfig[row.riesgo];
              return (
                <tr
                  key={row.estado}
                  style={{ borderBottom: `1px solid ${T.border}` }}
                  onMouseEnter={(e) =>
                    (e.currentTarget.style.background = "rgba(255,255,255,0.02)")
                  }
                  onMouseLeave={(e) =>
                    (e.currentTarget.style.background = "transparent")
                  }
                >
                  <td style={{ padding: "10px 14px", fontSize: "13px", color: T.text0, fontWeight: 500 }}>
                    {row.estado}
                  </td>
                  <td style={{ padding: "10px 14px", fontSize: "12px", color: T.text1, fontFamily: "Space Grotesk" }}>
                    {row.altitud.toLocaleString("es-MX")} m
                  </td>
                  <td style={{ padding: "10px 14px", fontSize: "12px", color: T.text2, fontFamily: "Space Grotesk" }}>
                    {row.lat}
                  </td>
                  <td style={{ padding: "10px 14px" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                      <div
                        style={{
                          flex: 1,
                          height: "4px",
                          background: T.bg4,
                          borderRadius: "2px",
                          maxWidth: "100px",
                        }}
                      >
                        <div
                          style={{
                            height: "100%",
                            width: `${(row.incidentes / maxInc) * 100}%`,
                            background: rc.color,
                            borderRadius: "2px",
                            transition: "width 0.3s",
                          }}
                        />
                      </div>
                      <span
                        style={{
                          fontSize: "12px",
                          fontWeight: 600,
                          color: T.text0,
                          fontFamily: "Space Grotesk",
                          minWidth: "28px",
                        }}
                      >
                        {row.incidentes}
                      </span>
                    </div>
                  </td>
                  <td style={{ padding: "10px 14px" }}>
                    <RiskBadge risk={row.riesgo} />
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
