import { useState } from "react";
import type { Platform } from "../types";
import { T, fmt } from "../theme";

export function DonutChart({ segments }: { segments: Platform[] }) {
  const total = segments.reduce((s, x) => s + x.incidents, 0);
  let angle = -90;
  const cx = 60;
  const cy = 60;
  const r = 50;
  const ir = 34;
  const slices = segments.map((seg) => {
    const pct = total > 0 ? seg.incidents / total : 0;
    const a = pct * 360;
    const rad1 = (angle * Math.PI) / 180;
    const rad2 = ((angle + a) * Math.PI) / 180;
    const x1 = cx + r * Math.cos(rad1);
    const y1 = cy + r * Math.sin(rad1);
    const x2 = cx + r * Math.cos(rad2);
    const y2 = cy + r * Math.sin(rad2);
    const ix1 = cx + ir * Math.cos(rad1);
    const iy1 = cy + ir * Math.sin(rad1);
    const ix2 = cx + ir * Math.cos(rad2);
    const iy2 = cy + ir * Math.sin(rad2);
    const large = a > 180 ? 1 : 0;
    const d = `M${x1},${y1} A${r},${r} 0 ${large},1 ${x2},${y2} L${ix2},${iy2} A${ir},${ir} 0 ${large},0 ${ix1},${iy1} Z`;
    angle += a;
    return { ...seg, d, pct };
  });
  const [hov, setHov] = useState<number | null>(null);
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "20px" }}>
      <svg width="120" height="120" viewBox="0 0 120 120">
        {slices.map((s, i) => (
          <path
            key={i}
            d={s.d}
            fill={s.color}
            opacity={hov === null || hov === i ? 1 : 0.4}
            style={{ cursor: "pointer", transition: "opacity 0.2s" }}
            onMouseEnter={() => setHov(i)}
            onMouseLeave={() => setHov(null)}
          />
        ))}
        <text
          x="60"
          y="56"
          textAnchor="middle"
          fontSize="14"
          fontWeight="700"
          fill={T.text0}
          fontFamily="Inter"
        >
          {hov !== null ? `${(slices[hov].pct * 100).toFixed(0)}%` : fmt(total)}
        </text>
        <text
          x="60"
          y="70"
          textAnchor="middle"
          fontSize="8"
          fill={T.text1}
          fontFamily="Space Grotesk"
        >
          {hov !== null ? slices[hov].name : "total"}
        </text>
      </svg>
      <div style={{ display: "flex", flexDirection: "column", gap: "8px", flex: 1 }}>
        {segments.map((s, i) => (
          <div
            key={i}
            style={{ display: "flex", alignItems: "center", gap: "8px", cursor: "default" }}
            onMouseEnter={() => setHov(i)}
            onMouseLeave={() => setHov(null)}
          >
            <div
              style={{
                width: "8px",
                height: "8px",
                borderRadius: "2px",
                background: s.color,
                flexShrink: 0,
              }}
            />
            <span style={{ fontSize: "12px", color: T.text1, flex: 1 }}>{s.name}</span>
            <span
              style={{
                fontSize: "12px",
                fontWeight: 600,
                color: T.text0,
                fontFamily: "Space Grotesk",
              }}
            >
              {s.pct}%
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
