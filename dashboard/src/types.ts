export type Risk = "CRÍTICO" | "ALTO" | "MEDIO" | "BAJO";
export type RiskFilter = "TODOS" | Risk;

export type PageId = "overview" | "incidents" | "settings";

export type IconName =
  | "shield" | "alert" | "users" | "block" | "globe" | "trend"
  | "eye" | "map" | "filter" | "activity" | "home" | "settings"
  | "report" | "pulse";

export interface Platform {
  name: string;
  incidents: number;
  pct: number;
  color: string;
}

export interface Incident {
  id: string;
  platform: string;
  type: string;
  risk: Risk;
  lat: string;
  lon: string;
  time: string;
  region: string;
}

export interface StateRow {
  estado: string;
  altitud: number;
  lat: string;
  incidentes: number;
  riesgo: Risk;
  ofensa: string;
}

export interface RiskStyle {
  bg: string;
  color: string;
  dot: string;
}

import { T } from "./theme";

export const riskConfig: Record<Risk, RiskStyle> = {
  "CRÍTICO": { bg: "rgba(255,77,77,0.18)", color: T.secondary, dot: T.secondary },
  "ALTO":    { bg: "rgba(255,193,7,0.18)", color: T.amber,     dot: T.amber },
  "MEDIO":   { bg: "rgba(33,150,243,0.18)", color: T.primary,  dot: T.primary },
  "BAJO":    { bg: "rgba(76,175,80,0.18)", color: T.green,     dot: T.green },
};
