import { useEffect, useState } from "react";
import { platform } from "@tauri-apps/plugin-os";

type Padding = {
  top: string;
  bottom: string;
  left: string;
  right: string;
};

const IOS_SIDES = {
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
  iosTopExtra = "1.5rem",
}: {
  children: React.ReactNode;
  className?: string;
  iosTopExtra?: string;
}) {
  const [isAndroid, setIsAndroid] = useState<boolean>(detectIsAndroid);

  useEffect(() => {
    setIsAndroid(detectIsAndroid());
  }, []);

  // Pin to the visual viewport so the on-screen keyboard can't push the
  // layout up and overlap the notch. When the keyboard opens, vv.height
  // shrinks → AppShell shrinks → flex-1 children (e.g. textarea) shrink,
  // so the focused input stays visible without iOS having to auto-scroll.
  useEffect(() => {
    const vv = window.visualViewport;
    if (!vv) return;
    const update = () => {
      const root = document.documentElement;
      root.style.setProperty("--vvh", `${vv.height}px`);
      root.style.setProperty("--vvtop", `${vv.offsetTop}px`);
    };
    vv.addEventListener("resize", update);
    vv.addEventListener("scroll", update);
    update();
    return () => {
      vv.removeEventListener("resize", update);
      vv.removeEventListener("scroll", update);
    };
  }, []);

  const padding: Padding = isAndroid
    ? ANDROID
    : {
        ...IOS_SIDES,
        top: `calc(env(safe-area-inset-top, 40px) + ${iosTopExtra})`,
      };

  return (
    <main
      className={`flex flex-col text-white ${className}`}
      style={{
        position: "fixed",
        top: "var(--vvtop, 0px)",
        left: 0,
        width: "100%",
        height: "var(--vvh, 100dvh)",
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
