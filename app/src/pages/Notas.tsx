import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArrowLeft, Plus, Trash2, NotebookPen } from "lucide-react";
import AppShell from "../components/AppShell";

type Note = {
  id: string;
  content: string;
  createdAt: number;
  updatedAt: number;
};

const STORAGE_KEY = "notas:v1";

function loadNotes(): Note[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.map((n) => ({
      id: String(n.id ?? crypto.randomUUID()),
      content: String(n.content ?? ""),
      createdAt: Number(n.createdAt ?? n.updatedAt ?? Date.now()),
      updatedAt: Number(n.updatedAt ?? Date.now()),
    }));
  } catch {
    return [];
  }
}

function getPreview(content: string): { title: string; body: string } {
  const trimmed = content.trim();
  if (!trimmed) return { title: "Nota nueva", body: "" };
  const lines = trimmed.split("\n");
  const title = lines[0].trim() || "Nota nueva";
  const body = lines
    .slice(1)
    .map((l) => l.trim())
    .filter(Boolean)
    .join(" · ");
  return { title: title.slice(0, 60), body: body.slice(0, 120) };
}

function timeAgo(ts: number): string {
  const diff = Date.now() - ts;
  const min = Math.floor(diff / 60_000);
  if (min < 1) return "ahora";
  if (min < 60) return `hace ${min} min`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `hace ${hr} h`;
  const day = Math.floor(hr / 24);
  if (day < 7) return `hace ${day} d`;
  return new Date(ts).toLocaleDateString("es-ES", {
    day: "numeric",
    month: "short",
  });
}

function countWords(content: string): number {
  const t = content.trim();
  if (!t) return 0;
  return t.split(/\s+/).filter(Boolean).length;
}

export default function Notas() {
  const [notes, setNotes] = useState<Note[]>(() => loadNotes());
  const [activeId, setActiveId] = useState<string | null>(null);

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(notes));
  }, [notes]);

  const sorted = [...notes].sort((a, b) => b.updatedAt - a.updatedAt);
  const active = activeId ? notes.find((n) => n.id === activeId) ?? null : null;

  function createNote() {
    const note: Note = {
      id: crypto.randomUUID(),
      content: "",
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };
    setNotes((prev) => [note, ...prev]);
    setActiveId(note.id);
  }

  function updateContent(content: string) {
    if (!activeId) return;
    setNotes((prev) =>
      prev.map((n) =>
        n.id === activeId ? { ...n, content, updatedAt: Date.now() } : n,
      ),
    );
  }

  function deleteActive() {
    if (!activeId) return;
    setNotes((prev) => prev.filter((n) => n.id !== activeId));
    setActiveId(null);
  }

  function exitEditor() {
    if (active && active.content.trim() === "") {
      setNotes((prev) => prev.filter((n) => n.id !== active.id));
    }
    setActiveId(null);
  }

  if (active) {
    const wc = countWords(active.content);
    const cc = active.content.length;
    return (
      <AppShell className="gap-4" iosTopExtra="0px">
        <header className="flex items-center justify-between gap-3 mt-4">
          <button
            type="button"
            aria-label="Volver"
            onClick={exitEditor}
            className="px-3 py-2 rounded-lg bg-white/10 hover:bg-white/20 active:bg-white/25 transition-colors flex items-center justify-center"
          >
            <ArrowLeft className="w-4 h-4" strokeWidth={2} />
          </button>
          <span className="text-xs text-white/55 tabular-nums">
            Editado {timeAgo(active.updatedAt)}
          </span>
          <button
            type="button"
            aria-label="Eliminar nota"
            onClick={deleteActive}
            className="px-3 py-2 rounded-lg bg-white/10 hover:bg-red-500/30 active:bg-red-500/40 text-white/85 transition-colors flex items-center justify-center"
          >
            <Trash2 className="w-4 h-4" strokeWidth={2} />
          </button>
        </header>

        <div className="flex-1 rounded-3xl bg-white/15 backdrop-blur-xl border border-white/20 shadow-[inset_0_1px_0_rgba(255,255,255,0.3),0_8px_24px_rgba(0,0,0,0.08)] overflow-hidden flex flex-col min-h-0">
          <textarea
            value={active.content}
            onChange={(e) => updateContent(e.target.value)}
            placeholder="Empieza a escribir aquí…"
            className="flex-1 bg-transparent px-6 py-5 text-white placeholder-white/35 outline-none resize-none leading-relaxed text-base"
          />
          <div className="px-6 py-3 border-t border-white/10 flex items-center justify-between text-[0.7rem] text-white/50 tabular-nums select-none">
            <span>
              {wc} palabra{wc !== 1 ? "s" : ""}
            </span>
            <span>
              {cc} carácter{cc !== 1 ? "es" : ""}
            </span>
          </div>
        </div>
      </AppShell>
    );
  }

  return (
    <AppShell className="gap-4" iosTopExtra="0px">
      <header className="flex items-center gap-3 mt-4">
        <Link
          to="/"
          aria-label="Volver"
          className="px-3 py-2 rounded-lg bg-white/10 hover:bg-white/20 transition-colors flex items-center justify-center"
        >
          <ArrowLeft className="w-4 h-4" strokeWidth={2} />
        </Link>
        <div className="flex-1 leading-tight">
          <h1 className="text-xl font-semibold">Notas</h1>
          <p className="text-[0.7rem] text-white/55 tabular-nums">
            {sorted.length} nota{sorted.length !== 1 ? "s" : ""}
          </p>
        </div>
        <button
          type="button"
          aria-label="Nueva nota"
          onClick={createNote}
          className="px-3 py-2 rounded-lg bg-white/20 hover:bg-white/30 active:bg-white/35 border border-white/25 shadow-[inset_0_1px_0_rgba(255,255,255,0.25)] transition-colors flex items-center justify-center"
        >
          <Plus className="w-4 h-4" strokeWidth={2.5} />
        </button>
      </header>

      <div className="flex-1 overflow-y-auto -mx-1 px-1 min-h-0">
        {sorted.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center text-center px-8">
            <div className="w-16 h-16 rounded-3xl bg-white/12 backdrop-blur-md border border-white/20 flex items-center justify-center mb-4 shadow-[inset_0_1px_0_rgba(255,255,255,0.3),0_4px_16px_rgba(0,0,0,0.08)]">
              <NotebookPen className="w-7 h-7 text-white/85" strokeWidth={1.5} />
            </div>
            <p className="text-base font-medium text-white/85">
              Tu mente, en blanco
            </p>
            <p className="text-sm mt-1 text-white/50 max-w-[16rem]">
              Toca el botón <span className="text-white/70">+</span> para crear
              tu primera nota
            </p>
          </div>
        ) : (
          <ul className="flex flex-col gap-2 pb-2">
            {sorted.map((n) => {
              const p = getPreview(n.content);
              return (
                <li key={n.id}>
                  <button
                    type="button"
                    onClick={() => setActiveId(n.id)}
                    className="w-full text-left rounded-2xl bg-white/10 hover:bg-white/15 active:bg-white/20 backdrop-blur-md border border-white/15 transition-all duration-150 px-4 py-3.5 flex flex-col gap-1 shadow-[inset_0_1px_0_rgba(255,255,255,0.2),0_2px_8px_rgba(0,0,0,0.06)]"
                  >
                    <div className="flex items-baseline justify-between gap-3">
                      <span className="font-medium text-white truncate">
                        {p.title}
                      </span>
                      <span className="text-[0.7rem] text-white/50 shrink-0 tabular-nums">
                        {timeAgo(n.updatedAt)}
                      </span>
                    </div>
                    <span className="text-sm text-white/55 truncate">
                      {p.body || "Sin contenido adicional"}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </AppShell>
  );
}
