// Página de eventos filtrados — muestra todo lo que la app filtró:
// texto/imagen, acción (block/warn), texto original, categorías, URL,
// timestamp y coordenadas (x,y,w,h) del elemento dentro de la WebView.
//
// "Acomodar" = ordenar por columna; "editar" = limpiar el buffer del
// server. El visor en lápiz pinta las coordenadas de cada evento
// como punto coloreado en un viewport de referencia (1280×800), lo cual
// da una vista espacial de dónde caen los filtrados en la página.

import { useMemo, useState } from "react";
import { useFilterEvents, type FilterEvent } from "../events";
import { T } from "../theme";

type SortKey = "time" | "y" | "kind" | "action";

const REF_VW = 1280;
const REF_VH = 800;

const ACTION_COLOR: Record<FilterEvent["action"], string> = {
  block: T.secondary,
  warn: T.amber,
  allow: T.green,
};

export function FilteredEventsPage() {
  const { events, error, clear } = useFilterEvents();
  const [sort, setSort] = useState<SortKey>("time");
  const [kindFilter, setKindFilter] = useState<"all" | "text" | "image">("all");

  const filtered = useMemo(() => {
    const filt =
      kindFilter === "all" ? events : events.filter((e) => e.kind === kindFilter);
    const arr = [...filt];
    switch (sort) {
      case "time":
        arr.sort((a, b) => b.timestamp_ms - a.timestamp_ms);
        break;
      case "y":
        arr.sort((a, b) => a.coords.y - b.coords.y);
        break;
      case "kind":
        arr.sort((a, b) => a.kind.localeCompare(b.kind));
        break;
      case "action":
        arr.sort((a, b) => a.action.localeCompare(b.action));
        break;
    }
    return arr;
  }, [events, sort, kindFilter]);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <Header
        total={events.length}
        kindFilter={kindFilter}
        setKindFilter={setKindFilter}
        sort={sort}
        setSort={setSort}
        onClear={clear}
        error={error}
      />

      <Card title="Mapa de coordenadas" sub={`Viewport de referencia ${REF_VW}×${REF_VH}`}>
        <CoordsCanvas events={filtered} />
      </Card>

      <Card title="Tabla de eventos" sub={`${filtered.length} de ${events.length}`}>
        <EventsTable events={filtered} />
      </Card>
    </div>
  );
}

function Header({
  total,
  kindFilter,
  setKindFilter,
  sort,
  setSort,
  onClear,
  error,
}: {
  total: number;
  kindFilter: "all" | "text" | "image";
  setKindFilter: (k: "all" | "text" | "image") => void;
  sort: SortKey;
  setSort: (s: SortKey) => void;
  onClear: () => void;
  error: string | null;
}) {
  return (
    <div
      style={{
        background: T.bg2,
        border: `1px solid ${T.border}`,
        borderRadius: 12,
        padding: 16,
        display: "flex",
        gap: 12,
        alignItems: "center",
        flexWrap: "wrap",
      }}
    >
      <div style={{ marginRight: "auto" }}>
        <div style={{ fontSize: 14, fontWeight: 600, color: T.text0 }}>
          Eventos filtrados
        </div>
        <div style={{ fontSize: 11, color: T.text2, marginTop: 2 }}>
          {total} eventos {error ? `· ⚠ ${error}` : "· en vivo"}
        </div>
      </div>

      <Select
        value={kindFilter}
        onChange={(v) => setKindFilter(v as typeof kindFilter)}
        options={[
          { v: "all", label: "Todos" },
          { v: "text", label: "Texto" },
          { v: "image", label: "Imagen" },
        ]}
      />

      <Select
        value={sort}
        onChange={(v) => setSort(v as SortKey)}
        options={[
          { v: "time", label: "Tiempo ↓" },
          { v: "y", label: "Coord Y ↑" },
          { v: "kind", label: "Tipo" },
          { v: "action", label: "Acción" },
        ]}
      />

      <button
        onClick={onClear}
        style={{
          background: T.secondaryDim,
          color: T.secondary,
          border: `1px solid ${T.secondary}33`,
          borderRadius: 8,
          padding: "8px 14px",
          fontSize: 12,
          fontWeight: 600,
          cursor: "pointer",
        }}
      >
        Limpiar
      </button>
    </div>
  );
}

function Select({
  value,
  onChange,
  options,
}: {
  value: string;
  onChange: (v: string) => void;
  options: { v: string; label: string }[];
}) {
  // Chevron SVG inline data URI — color T.text2 (#606060). Apariencia
  // nativa removida para que el dropdown respete el tema oscuro en todas
  // las plataformas.
  const chevron =
    "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='10' height='6' viewBox='0 0 10 6'><path d='M1 1l4 4 4-4' fill='none' stroke='%23A0A0A0' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'/></svg>\")";
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      style={{
        appearance: "none",
        WebkitAppearance: "none",
        MozAppearance: "none",
        background: `${T.bg3} ${chevron} no-repeat right 10px center`,
        color: T.text0,
        border: `1px solid ${T.border}`,
        borderRadius: 8,
        padding: "6px 28px 6px 12px",
        fontSize: 12,
        fontFamily: "inherit",
        fontWeight: 500,
        cursor: "pointer",
        outline: "none",
        colorScheme: "dark",
        minWidth: 110,
        transition: "border-color 120ms ease, background-color 120ms ease",
      }}
      onFocus={(e) => {
        e.currentTarget.style.borderColor = T.primary;
      }}
      onBlur={(e) => {
        e.currentTarget.style.borderColor = T.border;
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.background = `${T.bg4} ${chevron} no-repeat right 10px center`;
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = `${T.bg3} ${chevron} no-repeat right 10px center`;
      }}
    >
      {options.map((o) => (
        <option key={o.v} value={o.v} style={{ background: T.bg2, color: T.text0 }}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

function Card({
  title,
  sub,
  children,
}: {
  title: string;
  sub: string;
  children: React.ReactNode;
}) {
  return (
    <div
      style={{
        background: T.bg2,
        border: `1px solid ${T.border}`,
        borderRadius: 12,
        padding: 16,
      }}
    >
      <div style={{ marginBottom: 12 }}>
        <div style={{ fontSize: 13, fontWeight: 600, color: T.text0 }}>{title}</div>
        <div style={{ fontSize: 11, color: T.text2, marginTop: 2 }}>{sub}</div>
      </div>
      {children}
    </div>
  );
}

function CoordsCanvas({ events }: { events: FilterEvent[] }) {
  // Render a scaled-down "viewport" with one rect per event positioned at its
  // (x,y,w,h). Since events come from many pages with different viewport sizes,
  // we just use the reference viewport — anything off-screen gets clipped.
  return (
    <div
      style={{
        position: "relative",
        width: "100%",
        aspectRatio: `${REF_VW} / ${REF_VH}`,
        background: T.bg1,
        border: `1px dashed ${T.border}`,
        borderRadius: 8,
        overflow: "hidden",
      }}
    >
      {events.map((e) => {
        const left = (e.coords.x / REF_VW) * 100;
        const top = (e.coords.y / REF_VH) * 100;
        const w = (Math.max(e.coords.width, 4) / REF_VW) * 100;
        const h = (Math.max(e.coords.height, 4) / REF_VH) * 100;
        const color = ACTION_COLOR[e.action];
        return (
          <div
            key={e.id}
            title={`${e.kind} · ${e.action} · (${e.coords.x},${e.coords.y})`}
            style={{
              position: "absolute",
              left: `${left}%`,
              top: `${top}%`,
              width: `${w}%`,
              height: `${h}%`,
              background: `${color}55`,
              border: `1px solid ${color}`,
              borderRadius: 2,
            }}
          />
        );
      })}
    </div>
  );
}

function EventsTable({ events }: { events: FilterEvent[] }) {
  if (events.length === 0) {
    return (
      <div style={{ padding: 24, textAlign: "center", color: T.text2, fontSize: 13 }}>
        Sin eventos todavía. Navega en la app y los filtros llegarán aquí.
      </div>
    );
  }
  return (
    <div style={{ overflow: "auto", maxHeight: 480 }}>
      <table
        style={{
          width: "100%",
          borderCollapse: "collapse",
          fontSize: 12,
          color: T.text1,
        }}
      >
        <thead>
          <tr
            style={{
              position: "sticky",
              top: 0,
              background: T.bg2,
              fontSize: 10,
              textTransform: "uppercase",
              color: T.text2,
              letterSpacing: "0.05em",
            }}
          >
            <Th>Tipo</Th>
            <Th>Acción</Th>
            <Th>Original</Th>
            <Th>Filtrado</Th>
            <Th>Categorías</Th>
            <Th>x</Th>
            <Th>y</Th>
            <Th>w</Th>
            <Th>h</Th>
            <Th>URL</Th>
            <Th>Hora</Th>
          </tr>
        </thead>
        <tbody>
          {events.map((e) => (
            <tr key={e.id} style={{ borderTop: `1px solid ${T.border}` }}>
              <Td>{e.kind}</Td>
              <Td>
                <span
                  style={{
                    background: `${ACTION_COLOR[e.action]}22`,
                    color: ACTION_COLOR[e.action],
                    padding: "2px 8px",
                    borderRadius: 4,
                    fontWeight: 600,
                    textTransform: "uppercase",
                    fontSize: 10,
                  }}
                >
                  {e.action}
                </span>
              </Td>
              <Td style={{ maxWidth: 240 }}>{truncate(e.original, 80)}</Td>
              <Td style={{ maxWidth: 240, fontFamily: "monospace" }}>
                {truncate(e.filtered, 60)}
              </Td>
              <Td>{e.categories.join(", ") || "—"}</Td>
              <Td>{e.coords.x}</Td>
              <Td>{e.coords.y}</Td>
              <Td>{e.coords.width}</Td>
              <Td>{e.coords.height}</Td>
              <Td style={{ maxWidth: 200 }}>{truncate(e.url, 50)}</Td>
              <Td>{new Date(e.timestamp_ms).toLocaleTimeString("es-MX")}</Td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Th({ children }: { children: React.ReactNode }) {
  return <th style={{ textAlign: "left", padding: "8px 10px", fontWeight: 600 }}>{children}</th>;
}

function Td({
  children,
  style,
}: {
  children: React.ReactNode;
  style?: React.CSSProperties;
}) {
  return (
    <td
      style={{
        padding: "8px 10px",
        whiteSpace: "nowrap",
        overflow: "hidden",
        textOverflow: "ellipsis",
        ...style,
      }}
    >
      {children}
    </td>
  );
}

function truncate(s: string, n: number): string {
  if (s.length <= n) return s;
  return s.slice(0, n) + "…";
}
