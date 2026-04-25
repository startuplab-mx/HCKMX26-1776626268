import { useState } from "react";
import { T } from "../theme";

export function BarChart24h({ data }: { data: number[] }) {
  const max = Math.max(...data);
  const [hov, setHov] = useState<number | null>(null);
  return (
    <div style={{ position: "relative", width: "100%", height: "160px" }}>
      <svg
        width="100%"
        height="100%"
        viewBox={`0 0 ${data.length * 22} 140`}
        preserveAspectRatio="none"
      >
        {data.map((v, i) => {
          const barH = (v / max) * 110;
          const isHigh = v > max * 0.6;
          const isHov = hov === i;
          return (
            <rect
              key={i}
              x={i * 22 + 2}
              y={130 - barH}
              width={18}
              height={barH}
              rx="3"
              fill={
                isHigh
                  ? v === max
                    ? T.secondary
                    : T.primary
                  : "rgba(33,150,243,0.25)"
              }
              opacity={isHov ? 1 : 0.85}
              style={{ cursor: "pointer", transition: "opacity 0.15s" }}
              onMouseEnter={() => setHov(i)}
              onMouseLeave={() => setHov(null)}
            />
          );
        })}
      </svg>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          paddingTop: "4px",
        }}
      >
        {["00:00", "06:00", "12:00", "18:00", "23:59"].map((l) => (
          <span
            key={l}
            style={{ fontSize: "10px", color: T.text2, fontFamily: "Space Grotesk" }}
          >
            {l}
          </span>
        ))}
      </div>
      {hov !== null && (
        <div
          style={{
            position: "absolute",
            top: 0,
            left: `${(hov / data.length) * 100}%`,
            background: T.bg3,
            border: `1px solid ${T.border}`,
            borderRadius: "6px",
            padding: "4px 8px",
            fontSize: "11px",
            whiteSpace: "nowrap",
            transform: "translateX(-50%)",
            pointerEvents: "none",
            fontFamily: "Space Grotesk",
            color: T.text0,
          }}
        >
          {String(hov).padStart(2, "0")}:00 —{" "}
          <strong style={{ color: T.primary }}>{data[hov]}</strong> alertas
        </div>
      )}
    </div>
  );
}
