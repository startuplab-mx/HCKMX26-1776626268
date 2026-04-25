import "./App.css";
import { Compass, Calculator, StickyNote } from "lucide-react";
import { FaFacebookF, FaInstagram } from "react-icons/fa6";
import App from "./components/App";
import AppShell from "./components/AppShell";

export default function Desktop() {
  return (
    <AppShell>
      <div className="flex flex-wrap items-center justify-center gap-4">
        <App name="Navegador" nav="/navegador">
          <Compass className="w-8 h-8" strokeWidth={1.5} />
        </App>
        <App name="Facebook" nav="/facebook">
          <FaFacebookF className="w-7 h-7" />
        </App>
        <App name="Instagram" nav="/instagram">
          <FaInstagram className="w-8 h-8" />
        </App>
        <App name="Calculadora" nav="/calculadora">
          <Calculator className="w-8 h-8" strokeWidth={1.5} />
        </App>
        <App name="Notas" nav="/notas">
          <StickyNote className="w-8 h-8" strokeWidth={1.5} />
        </App>
      </div>
    </AppShell>
  );
}
