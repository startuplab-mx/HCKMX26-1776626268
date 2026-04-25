import { T } from "../theme";
import type { RiskFilter } from "../types";

const OPTIONS: RiskFilter[] = ["TODOS", "CRÍTICO", "ALTO", "MEDIO", "BAJO"];

interface Props {
  value: RiskFilter;
  onChange: (v: RiskFilter) => void;
}

export function RiskFilterButtons({ value, onChange }: Props) {
  return (
    <div style={{ display: "flex", gap: "6px" }}>
      {OPTIONS.map((r) => (
        <button
          key={r}
          onClick={() => onChange(r)}
          style={{
            padding: "4px 10px",
            borderRadius: "6px",
            border: `1px solid ${value === r ? T.primary : T.border}`,
            background: value === r ? T.primaryDim : "transparent",
            color: value === r ? T.primary : T.text2,
            fontSize: "10px",
            fontFamily: "Space Grotesk",
            fontWeight: 600,
            cursor: "pointer",
            letterSpacing: "0.04em",
          }}
        >
          {r}
        </button>
      ))}
    </div>
  );
}
