import { T } from "../theme";
import type { IconName } from "../types";
import { Icon } from "./Icon";
import { Sparkline } from "./Sparkline";

interface Props {
  label: string;
  value: string;
  sub: string;
  subColor?: string;
  icon: IconName;
  accent: string;
  spark?: number[];
}

export function KpiCard({
  label,
  value,
  sub,
  subColor,
  icon,
  accent,
  spark,
}: Props) {
  return (
    <div
      style={{
        background: T.bg2,
        border: `1px solid ${T.border}`,
        borderRadius: "12px",
        padding: "20px",
        display: "flex",
        flexDirection: "column",
        gap: "8px",
        flex: 1,
        minWidth: 0,
        position: "relative",
        overflow: "hidden",
      }}
    >
      <div
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          height: "2px",
          background: accent,
          borderRadius: "12px 12px 0 0",
        }}
      />
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "flex-start",
        }}
      >
        <span
          style={{
            fontSize: "11px",
            fontFamily: "Space Grotesk",
            fontWeight: 500,
            color: T.text1,
            letterSpacing: "0.08em",
            textTransform: "uppercase",
          }}
        >
          {label}
        </span>
        <div style={{ background: `${accent}22`, padding: "6px", borderRadius: "8px" }}>
          <Icon name={icon} size={14} color={accent} />
        </div>
      </div>
      <div
        style={{
          fontSize: "28px",
          fontWeight: 700,
          color: T.text0,
          letterSpacing: "-0.02em",
          lineHeight: 1,
        }}
      >
        {value}
      </div>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "flex-end",
        }}
      >
        <span style={{ fontSize: "12px", color: subColor || T.text1 }}>{sub}</span>
        {spark && <Sparkline data={spark} color={accent} />}
      </div>
    </div>
  );
}
