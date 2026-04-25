import { T } from "../theme";
import { weekDays } from "../data/mock";
import {
  useHourly,
  useIncidents,
  usePlatforms,
  useStates,
  useWeeklyTrend,
} from "../data/hooks";
import { BarChart24h } from "../components/BarChart24h";
import { DonutChart } from "../components/DonutChart";
import { IncidentsTable } from "../components/IncidentsTable";
import { KpiCard } from "../components/KpiCard";
import { LiveFeed } from "../components/LiveFeed";
import { StatesRawTable } from "../components/StatesRawTable";
import { StatesTable } from "../components/StatesTable";
import { WeekChart } from "../components/WeekChart";

export function OverviewPage() {
  const incidents = useIncidents();
  const states = useStates();
  const platforms = usePlatforms();
  const hourly = useHourly();
  const weekly = useWeeklyTrend();

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
      {/* KPI Row — values are still hard-coded; wire to real metrics later. */}
      <div style={{ display: "flex", gap: "12px", flexShrink: 0 }}>
        <KpiCard
          label="Endpoints Activos"
          value="1,248,302"
          sub="↑ +4.2% este mes"
          subColor={T.green}
          icon="users"
          accent={T.primary}
          spark={weekly.data ?? []}
        />
        <KpiCard
          label="Requests Bloqueadas"
          value="42,912"
          sub="⚠ Spike en api-gateway"
          subColor={T.secondary}
          icon="block"
          accent={T.secondary}
          spark={[40, 55, 48, 62, 58, 70, 65]}
        />
        <KpiCard
          label="Alertas Activas"
          value="284"
          sub="↑ 18 en la última hora"
          subColor={T.amber}
          icon="alert"
          accent={T.amber}
          spark={[20, 30, 25, 40, 35, 45, 42]}
        />
      </div>

      <div style={{ display: "flex", gap: "12px", flexShrink: 0 }}>
        <Card flex={2} title="Picos de Latencia" sub="Últimas 24 horas · hora local UTC" rightAccent="TENDENCIA">
          {hourly.data && <BarChart24h data={hourly.data} />}
        </Card>

        <Card flex={1} title="Servicios" sub="Distribución por servicio">
          {platforms.data && <DonutChart segments={platforms.data} />}
        </Card>
      </div>

      <div style={{ display: "flex", gap: "12px", flexShrink: 0 }}>
        <Card flex={1} title="Tendencia Semanal" sub="Alertas últimos 7 días">
          {weekly.data && <WeekChart data={weekly.data} labels={weekDays} />}
          <WeeklyStats data={weekly.data ?? []} />
        </Card>

        <Card flex={1} title="Feed en Vivo" sub="Eventos entrantes en tiempo real" rightAccent="LIVE">
          {incidents.data && <LiveFeed seed={incidents.data} />}
        </Card>
      </div>

      {states.data && <StatesRawTable rows={states.data} />}
      {states.data && <StatesTable rows={states.data} />}
      {incidents.data && <IncidentsTable incidents={incidents.data} />}
    </div>
  );
}

function Card({
  flex,
  title,
  sub,
  rightAccent,
  children,
}: {
  flex: number;
  title: string;
  sub: string;
  rightAccent?: string;
  children: React.ReactNode;
}) {
  return (
    <div
      style={{
        flex,
        background: T.bg2,
        border: `1px solid ${T.border}`,
        borderRadius: "12px",
        padding: "16px",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: "14px",
        }}
      >
        <div>
          <div style={{ fontSize: "13px", fontWeight: 600, color: T.text0 }}>
            {title}
          </div>
          <div style={{ fontSize: "11px", color: T.text2, marginTop: "2px" }}>
            {sub}
          </div>
        </div>
        {rightAccent && (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "6px",
              fontSize: "11px",
              color: T.primary,
            }}
          >
            <span
              style={{
                width: "8px",
                height: "8px",
                borderRadius: "50%",
                background: T.primary,
                display: "inline-block",
              }}
            />
            {rightAccent}
          </div>
        )}
      </div>
      {children}
    </div>
  );
}

function WeeklyStats({ data }: { data: number[] }) {
  if (data.length === 0) return null;
  const peak = Math.max(...data);
  const avg = Math.round(data.reduce((a, b) => a + b, 0) / data.length);
  const last = data[data.length - 1];
  const prev = data[data.length - 2] ?? last;
  const delta = prev > 0 ? Math.round(((last - prev) / prev) * 100) : 0;
  const arrow = delta >= 0 ? "↑" : "↓";

  return (
    <div style={{ marginTop: "10px", display: "flex", gap: "12px" }}>
      <Stat label="PICO" value={String(peak)} color={peak === last ? T.secondary : T.text0} />
      <Stat label="PROMEDIO" value={String(avg)} color={T.text0} />
      <Stat label="DELTA" value={`${arrow}${Math.abs(delta)}%`} color={T.amber} />
    </div>
  );
}

function Stat({ label, value, color }: { label: string; value: string; color: string }) {
  return (
    <div>
      <div style={{ fontSize: "10px", color: T.text2, fontFamily: "Space Grotesk" }}>
        {label}
      </div>
      <div style={{ fontSize: "16px", fontWeight: 700, color }}>{value}</div>
    </div>
  );
}
