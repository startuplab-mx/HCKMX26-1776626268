import type { Incident } from "../types";
import { riskConfig } from "../types";
import { T } from "../theme";
import { useLiveFeed } from "../data/hooks";
import { PulseDot } from "./PulseDot";

interface Props {
  seed: Incident[];
}

export function LiveFeed({ seed }: Props) {
  const feed = useLiveFeed(seed);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
      {feed.map((item, i) => {
        const rc = riskConfig[item.risk];
        return (
          <div
            key={`${item.id}-${i}`}
            style={{
              display: "flex",
              alignItems: "center",
              gap: "10px",
              padding: "8px 10px",
              borderRadius: "8px",
              background: i === 0 ? rc.bg : "transparent",
              border:
                i === 0 ? `1px solid ${rc.color}33` : "1px solid transparent",
              transition: "all 0.3s",
            }}
          >
            <PulseDot color={rc.dot} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: "12px", color: T.text0, fontWeight: 500 }}>
                {item.type}
              </div>
              <div style={{ fontSize: "10px", color: T.text2, marginTop: "1px" }}>
                {item.region} · {item.platform}
              </div>
            </div>
            <span
              style={{
                fontSize: "10px",
                color: rc.color,
                fontFamily: "Space Grotesk",
                fontWeight: 600,
              }}
            >
              {item.risk}
            </span>
          </div>
        );
      })}
    </div>
  );
}
