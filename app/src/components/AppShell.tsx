import { useEffect, useState } from "react";
import { platform } from "@tauri-apps/plugin-os";

type Padding = {
  top: string;
  bottom: string;
  left: string;
  right: string;
};

const IOS_OR_DEFAULT: Padding = {
  top: "max(0.75rem, env(safe-area-inset-top), 40px)",
  bottom: "max(0.75rem, env(safe-area-inset-bottom), 16px)",
  left: "max(0.75rem, env(safe-area-inset-left), 16px)",
  right: "max(0.75rem, env(safe-area-inset-right), 16px)",
};

const ANDROID: Padding = {
  top: "35px",
  bottom: "16px",
  left: "16px",
  right: "16px",
};

function detectIsAndroid(): boolean {
  try {
    return platform() === "android";
  } catch {
    return (
      typeof navigator !== "undefined" && /android/i.test(navigator.userAgent)
    );
  }
}

export default function AppShell({
  children,
  className = "",
}: {
  children: React.ReactNode;
  className?: string;
}) {
  const [padding, setPadding] = useState<Padding>(() =>
    detectIsAndroid() ? ANDROID : IOS_OR_DEFAULT,
  );

  useEffect(() => {
    setPadding(detectIsAndroid() ? ANDROID : IOS_OR_DEFAULT);
  }, []);

  return (
    <main
      className={`h-dvh flex flex-col gap-2 text-white ${className}`}
      style={{
        paddingTop: padding.top,
        paddingBottom: padding.bottom,
        paddingLeft: padding.left,
        paddingRight: padding.right,
      }}
    >
      {children}
    </main>
  );
}
