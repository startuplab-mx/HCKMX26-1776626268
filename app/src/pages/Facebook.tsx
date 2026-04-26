import { Link } from "react-router-dom";
import { ArrowLeft } from "lucide-react";
import AppShell from "../components/AppShell";
import { useEmbeddedBrowser } from "../browserPane";

const FACEBOOK_URL = "https://m.facebook.com/";

export default function Facebook() {
  const { paneRef, navError } = useEmbeddedBrowser({
    owner: "facebook",
    url: FACEBOOK_URL,
  });

  return (
    <AppShell className="gap-3" iosTopExtra="0px">
      <header className="flex items-center gap-3 mt-4">
        <Link
          to="/"
          aria-label="Volver"
          className="px-3 py-2 rounded-lg bg-white/10 hover:bg-white/20 transition-colors flex items-center justify-center"
        >
          <ArrowLeft className="w-4 h-4" strokeWidth={2} />
        </Link>
        <h1 className="text-xl font-semibold">Facebook</h1>
      </header>

      {navError && (
        <div className="mb-3 px-3 py-2 rounded-lg bg-red-500/20 text-red-100 text-sm">
          {navError}
        </div>
      )}

      <div
        ref={paneRef}
        className="flex-1 rounded-xl bg-white/5 backdrop-blur-sm"
      />
    </AppShell>
  );
}
