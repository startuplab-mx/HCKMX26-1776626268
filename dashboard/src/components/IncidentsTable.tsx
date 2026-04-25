import { useState } from "react";
import { T } from "../theme";
import type { Incident, RiskFilter } from "../types";
import { RiskBadge } from "./RiskBadge";
import { RiskFilterButtons } from "./RiskFilter";
import { ExcelExportButton, buildExcelXml } from "./ExcelExportButton";

const HEADERS = ["ID", "Región", "Servicio", "Tipo", "Severidad", "Latitud", "Longitud", "Tiempo"];

interface Props {
  incidents: Incident[];
}

export function IncidentsTable({ incidents }: Props) {
  const [selected, setSelected] = useState<string | null>(null);
  const [filter, setFilter] = useState<RiskFilter>("TODOS");

  const filtered =
    filter === "TODOS" ? incidents : incidents.filter((i) => i.risk === filter);

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
          flexShrink: 0,
        }}
      >
        <div>
          <span style={{ fontSize: "13px", fontWeight: 600, color: T.text0 }}>
            Eventos Recientes
          </span>
          <span style={{ fontSize: "11px", color: T.text2, marginLeft: "8px" }}>
            · {filtered.length} eventos activos
          </span>
        </div>
        <div style={{ display: "flex", gap: "6px", alignItems: "center" }}>
          <RiskFilterButtons value={filter} onChange={setFilter} />
          <div
            style={{
              width: "1px",
              height: "20px",
              background: T.border,
              margin: "0 4px",
            }}
          />
          <ExcelExportButton
            filenamePrefix="sentinel_eventos"
            buildXml={() =>
              buildExcelXml(
                "Eventos",
                [
                  "ID",
                  "Región",
                  "Servicio",
                  "Tipo de Evento",
                  "Severidad",
                  "Latitud",
                  "Longitud",
                  "Tiempo",
                ],
                filtered.map((i) => [
                  i.id,
                  i.region,
                  i.platform,
                  i.type,
                  i.risk,
                  i.lat,
                  i.lon,
                  i.time,
                ])
              )
            }
          />
        </div>
      </div>
      <div style={{ overflow: "auto", flex: 1 }}>
        <table style={{ width: "100%", borderCollapse: "collapse" }}>
          <thead>
            <tr style={{ borderBottom: `1px solid ${T.border}` }}>
              {HEADERS.map((h) => (
                <th
                  key={h}
                  style={{
                    padding: "8px 12px",
                    textAlign: "left",
                    fontSize: "10px",
                    fontFamily: "Space Grotesk",
                    color: T.text2,
                    fontWeight: 500,
                    letterSpacing: "0.07em",
                    textTransform: "uppercase",
                    whiteSpace: "nowrap",
                    background: T.bg2,
                  }}
                >
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {filtered.map((inc) => (
              <IncidentRow
                key={inc.id}
                inc={inc}
                selected={selected === inc.id}
                onClick={() =>
                  setSelected((curr) => (curr === inc.id ? null : inc.id))
                }
              />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

interface RowProps {
  inc: Incident;
  selected: boolean;
  onClick: () => void;
}

function IncidentRow({ inc, selected, onClick }: RowProps) {
  return (
    <tr
      onClick={onClick}
      style={{
        background: selected ? "rgba(33,150,243,0.07)" : "transparent",
        cursor: "pointer",
        transition: "background 0.15s",
        borderBottom: `1px solid ${T.border}`,
      }}
      onMouseEnter={(e) => {
        if (!selected) e.currentTarget.style.background = "rgba(255,255,255,0.02)";
      }}
      onMouseLeave={(e) => {
        if (!selected) e.currentTarget.style.background = "transparent";
      }}
    >
      <td style={{ padding: "10px 12px", fontSize: "12px", fontFamily: "Space Grotesk", color: T.text1, whiteSpace: "nowrap" }}>
        {inc.id}
      </td>
      <td style={{ padding: "10px 12px", fontSize: "12px", color: T.text0 }}>{inc.region}</td>
      <td style={{ padding: "10px 12px", fontSize: "12px", color: T.text1 }}>{inc.platform}</td>
      <td style={{ padding: "10px 12px", fontSize: "12px", color: T.text0 }}>{inc.type}</td>
      <td style={{ padding: "10px 12px" }}>
        <RiskBadge risk={inc.risk} />
      </td>
      <td style={{ padding: "10px 12px", fontSize: "11px", color: T.text2, fontFamily: "Space Grotesk", whiteSpace: "nowrap" }}>
        {inc.lat}
      </td>
      <td style={{ padding: "10px 12px", fontSize: "11px", color: T.text2, fontFamily: "Space Grotesk", whiteSpace: "nowrap" }}>
        {inc.lon}
      </td>
      <td style={{ padding: "10px 12px", fontSize: "11px", color: T.text2, whiteSpace: "nowrap" }}>
        {inc.time}
      </td>
    </tr>
  );
}
