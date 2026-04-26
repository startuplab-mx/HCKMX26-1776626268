import { useState, type ReactNode } from "react";
import "./App.css";
import { Header } from "./components/Header";
import { Sidebar } from "./components/Sidebar";
import { FilteredEventsPage } from "./pages/FilteredEventsPage";
import { PlaceholderPage } from "./pages/PlaceholderPage";
import { useFilterEvents } from "./events";
import { T } from "./theme";
import type { PageId } from "./types";

function App() {
  const [page, setPage] = useState<PageId>("incidents");
  const { events } = useFilterEvents();
  const criticalCount = events.filter((e) => e.action === "block").length;

  const PAGES: Record<PageId, ReactNode> = {
    overview: <PlaceholderPage title="Overview" icon="home" />,
    incidents: <FilteredEventsPage />,
    settings: <PlaceholderPage title="Configuración del Sistema" icon="settings" />,
  };

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
