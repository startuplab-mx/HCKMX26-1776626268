import { T } from "../theme";
import type { IconName } from "../types";
import { Icon } from "../components/Icon";

interface Props {
  title: string;
  icon: IconName;
}

export function PlaceholderPage({ title, icon }: Props) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        height: "100%",
        gap: "16px",
        color: T.text2,
      }}
    >
      <div
        style={{
          background: T.bg3,
          borderRadius: "16px",
          padding: "20px",
          border: `1px solid ${T.border}`,
        }}
      >
        <Icon name={icon} size={32} color={T.text2} />
      </div>
      <div style={{ fontSize: "14px", fontWeight: 500 }}>{title}</div>
      <div style={{ fontSize: "12px", color: T.text2 }}>Módulo en construcción</div>
    </div>
  );
}
