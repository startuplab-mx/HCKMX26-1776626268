import { useState, type ReactNode } from "react";
import "./App.css";
import { Header } from "./components/Header";
import { Sidebar } from "./components/Sidebar";
import { OverviewPage } from "./pages/OverviewPage";
import { PlaceholderPage } from "./pages/PlaceholderPage";
import { useIncidents } from "./data/hooks";
import { T } from "./theme";
import type { PageId } from "./types";

const PAGES: Record<PageId, ReactNode> = {
  overview: <OverviewPage />,
  incidents: <PlaceholderPage title="Gestión de Eventos" icon="alert" />,
  settings: <PlaceholderPage title="Configuración del Sistema" icon="settings" />,
};

function App() {
  const [page, setPage] = useState<PageId>("overview");
  const incidents = useIncidents();
  const criticalCount =
    incidents.data?.filter((i) => i.risk === "CRÍTICO").length ?? 0;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100vh",
        overflow: "hidden",
      }}
    >
      <Header page={page} criticalCount={criticalCount} />
      <div style={{ display: "flex", flex: 1, overflow: "hidden" }}>
        <Sidebar active={page} setActive={setPage} />
        <main
          style={{
            flex: 1,
            overflow: "auto",
            padding: "16px",
            background: T.bg0,
          }}
        >
          {PAGES[page]}
        </main>
      </div>
    </div>
  );
}

export default App;
