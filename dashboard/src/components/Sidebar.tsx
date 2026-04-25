import { T } from "../theme";
import type { IconName, PageId } from "../types";
import { Icon } from "./Icon";

interface NavItem {
  id: PageId;
  icon: IconName;
  label: string;
}

const NAV: NavItem[] = [
  { id: "overview", icon: "home", label: "Overview" },
  { id: "incidents", icon: "alert", label: "Eventos" },
  { id: "settings", icon: "settings", label: "Config" },
];

interface Props {
  active: PageId;
  setActive: (p: PageId) => void;
}

export function Sidebar({ active, setActive }: Props) {
  return (
    <aside
      style={{
        width: "56px",
        background: T.bg1,
        borderRight: `1px solid ${T.border}`,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        padding: "16px 0",
        gap: "4px",
        flexShrink: 0,
      }}
    >
      <div
        style={{
          marginBottom: "20px",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <div
          style={{
            background: T.primaryDim,
            borderRadius: "10px",
            padding: "8px",
            border: `1px solid rgba(33,150,243,0.3)`,
          }}
        >
          <Icon name="shield" size={18} color={T.primary} />
        </div>
      </div>
      {NAV.map((n) => (
        <button
          key={n.id}
          onClick={() => setActive(n.id)}
          title={n.label}
          style={{
            width: "40px",
            height: "40px",
            borderRadius: "10px",
            border: "none",
            background: active === n.id ? T.primaryDim : "transparent",
            cursor: "pointer",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            transition: "background 0.15s",
            color: active === n.id ? T.primary : T.text2,
          }}
        >
          <Icon
            name={n.icon}
            size={16}
            color={active === n.id ? T.primary : T.text2}
          />
        </button>
      ))}
      <div style={{ marginTop: "auto" }}>
        <div
          style={{
            width: "28px",
            height: "28px",
            borderRadius: "50%",
            background: "linear-gradient(135deg, #2196F3, #1565C0)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: "11px",
            fontWeight: 600,
            color: "#fff",
          }}
        >
          A
        </div>
      </div>
    </aside>
  );
}
