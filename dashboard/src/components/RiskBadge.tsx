import { riskConfig, type Risk } from "../types";

export function RiskBadge({ risk }: { risk: Risk }) {
  const rc = riskConfig[risk];
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "5px",
        background: rc.bg,
        color: rc.color,
        fontSize: "10px",
        fontWeight: 600,
        fontFamily: "Space Grotesk",
        letterSpacing: "0.06em",
        padding: "3px 8px",
        borderRadius: "4px",
        textTransform: "uppercase",
      }}
    >
      <span
        style={{
          width: "5px",
          height: "5px",
          borderRadius: "50%",
          background: rc.dot,
          display: "inline-block",
        }}
      />
      {risk}
    </span>
  );
}
