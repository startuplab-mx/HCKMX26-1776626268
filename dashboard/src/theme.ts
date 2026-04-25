export const T = {
  bg0: "#0A0A0A",
  bg1: "#111111",
  bg2: "#161616",
  bg3: "#1E1E1E",
  bg4: "#252525",
  border: "rgba(255,255,255,0.06)",
  primary: "#2196F3",
  primaryDim: "rgba(33,150,243,0.15)",
  secondary: "#FF4D4D",
  secondaryDim: "rgba(255,77,77,0.15)",
  amber: "#FFC107",
  amberDim: "rgba(255,193,7,0.15)",
  green: "#4CAF50",
  greenDim: "rgba(76,175,80,0.15)",
  text0: "#F0F0F0",
  text1: "#A0A0A0",
  text2: "#606060",
} as const;

export const fmt = (n: number) => n.toLocaleString("es-MX");
