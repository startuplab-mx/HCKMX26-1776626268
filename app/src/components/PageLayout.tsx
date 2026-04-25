import { Link } from "react-router-dom";
import { ArrowLeft } from "lucide-react";
import AppShell from "./AppShell";

export default function PageLayout({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <AppShell>
      <header className="flex items-center gap-3 mb-4">
        <Link
          to="/"
          aria-label="Volver"
          className="px-3 py-2 rounded-lg bg-white/10 hover:bg-white/20 transition-colors flex items-center justify-center"
        >
          <ArrowLeft className="w-4 h-4" strokeWidth={2} />
        </Link>
        <h1 className="text-xl font-semibold">{title}</h1>
      </header>
      <section className="rounded-2xl bg-white/10 backdrop-blur-sm p-5 flex-1 overflow-auto">
        {children}
      </section>
    </AppShell>
  );
}
