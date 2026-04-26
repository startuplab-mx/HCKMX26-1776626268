import { useState } from "react";
import { Link } from "react-router-dom";
import { ArrowLeft } from "lucide-react";
import AppShell from "../components/AppShell";

type Op = "+" | "−" | "×" | "÷";

const MAX_LEN = 12;

function compute(a: number, b: number, op: Op): number {
  switch (op) {
    case "+":
      return a + b;
    case "−":
      return a - b;
    case "×":
      return a * b;
    case "÷":
      return b === 0 ? NaN : a / b;
  }
}

function formatNumber(n: number): string {
  if (!Number.isFinite(n)) return "Error";
  if (Math.abs(n) >= 1e12 || (Math.abs(n) < 1e-6 && n !== 0)) {
    return n.toExponential(6).replace("+", "");
  }
  const s = parseFloat(n.toPrecision(12)).toString();
  return s.length > MAX_LEN ? n.toPrecision(8) : s;
}

function displaySize(s: string): string {
  if (s.length <= 7) return "text-7xl";
  if (s.length <= 9) return "text-6xl";
  if (s.length <= 11) return "text-5xl";
  return "text-4xl";
}

export default function Calculadora() {
  const [display, setDisplay] = useState("0");
  const [acc, setAcc] = useState<number | null>(null);
  const [op, setOp] = useState<Op | null>(null);
  const [resetNext, setResetNext] = useState(false);

  function inputDigit(d: string) {
    if (display === "Error") {
      setDisplay(d);
      setAcc(null);
      setOp(null);
      setResetNext(false);
      return;
    }
    if (resetNext || display === "0") {
      setDisplay(d);
      setResetNext(false);
      return;
    }
    if (display.replace("-", "").replace(".", "").length >= MAX_LEN) return;
    setDisplay(display + d);
  }

  function inputDot() {
    if (display === "Error") return;
    if (resetNext) {
      setDisplay("0.");
      setResetNext(false);
      return;
    }
    if (!display.includes(".")) setDisplay(display + ".");
  }

  function setOperator(next: Op) {
    if (display === "Error") return;
    const current = parseFloat(display);
    if (acc !== null && op !== null && !resetNext) {
      const result = compute(acc, current, op);
      setAcc(result);
      setDisplay(formatNumber(result));
    } else {
      setAcc(current);
    }
    setOp(next);
    setResetNext(true);
  }

  function equals() {
    if (acc === null || op === null) return;
    const current = parseFloat(display);
    const result = compute(acc, current, op);
    setDisplay(formatNumber(result));
    setAcc(null);
    setOp(null);
    setResetNext(true);
  }

  function clearAll() {
    setDisplay("0");
    setAcc(null);
    setOp(null);
    setResetNext(false);
  }

  function percent() {
    if (display === "Error") return;
    setDisplay(formatNumber(parseFloat(display) / 100));
  }

  function toggleSign() {
    if (display === "0" || display === "Error") return;
    setDisplay(display.startsWith("-") ? display.slice(1) : "-" + display);
  }

  const showAC = display !== "0" || acc !== null || op !== null;

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
        <h1 className="text-xl font-semibold">Calculadora</h1>
      </header>

      <div className="flex-1 flex flex-col justify-end gap-6 min-h-0 overflow-hidden">
        <div className="text-right select-none">
          <div className="text-white/45 text-base tabular-nums font-light min-h-[1.5rem] truncate">
            {acc !== null && op
              ? `${formatNumber(acc)} ${op}`
              : " "}
          </div>
          <div
            className={`text-white tabular-nums font-extralight tracking-tight truncate leading-none ${displaySize(
              display,
            )}`}
          >
            {display}
          </div>
        </div>

        <div className="grid grid-cols-4 grid-rows-5 gap-3 w-full aspect-[4/5]">
          <Btn variant="util" onClick={clearAll}>
            {showAC ? "AC" : "C"}
          </Btn>
          <Btn variant="util" onClick={toggleSign}>±</Btn>
          <Btn variant="util" onClick={percent}>%</Btn>
          <Btn
            variant="op"
            active={op === "÷" && resetNext}
            onClick={() => setOperator("÷")}
          >
            ÷
          </Btn>

          <Btn onClick={() => inputDigit("7")}>7</Btn>
          <Btn onClick={() => inputDigit("8")}>8</Btn>
          <Btn onClick={() => inputDigit("9")}>9</Btn>
          <Btn
            variant="op"
            active={op === "×" && resetNext}
            onClick={() => setOperator("×")}
          >
            ×
          </Btn>

          <Btn onClick={() => inputDigit("4")}>4</Btn>
          <Btn onClick={() => inputDigit("5")}>5</Btn>
          <Btn onClick={() => inputDigit("6")}>6</Btn>
          <Btn
            variant="op"
            active={op === "−" && resetNext}
            onClick={() => setOperator("−")}
          >
            −
          </Btn>

          <Btn onClick={() => inputDigit("1")}>1</Btn>
          <Btn onClick={() => inputDigit("2")}>2</Btn>
          <Btn onClick={() => inputDigit("3")}>3</Btn>
          <Btn
            variant="op"
            active={op === "+" && resetNext}
            onClick={() => setOperator("+")}
          >
            +
          </Btn>

          <Btn span={2} onClick={() => inputDigit("0")}>0</Btn>
          <Btn onClick={inputDot}>,</Btn>
          <Btn variant="eq" onClick={equals}>=</Btn>
        </div>
      </div>
    </AppShell>
  );
}

type Variant = "digit" | "util" | "op" | "eq";

function Btn({
  children,
  onClick,
  variant = "digit",
  active = false,
  span = 1,
}: {
  children: React.ReactNode;
  onClick: () => void;
  variant?: Variant;
  active?: boolean;
  span?: 1 | 2;
}) {
  const variants: Record<Variant, string> = {
    digit:
      "bg-white/10 hover:bg-white/15 active:bg-white/20 text-white text-3xl",
    util:
      "bg-white/25 hover:bg-white/35 active:bg-white/40 text-white text-xl font-medium",
    op: active
      ? "bg-white text-purple-700 text-3xl font-normal shadow-[0_0_24px_rgba(255,255,255,0.35)]"
      : "bg-white/15 hover:bg-white/25 active:bg-white/30 text-white text-3xl",
    eq:
      "bg-gradient-to-br from-white/45 to-white/25 hover:from-white/55 hover:to-white/35 active:from-white/65 active:to-white/45 text-white text-3xl",
  };

  return (
    <button
      type="button"
      onClick={onClick}
      className={`
        ${variants[variant]}
        ${span === 2 ? "col-span-2" : ""}
        rounded-full backdrop-blur-md border border-white/20
        font-light select-none
        transition-all duration-150 ease-out
        active:scale-[0.94]
        flex items-center justify-center
        shadow-[inset_0_1px_0_rgba(255,255,255,0.25),0_2px_8px_rgba(0,0,0,0.08)]
      `}
    >
      {children}
    </button>
  );
}
