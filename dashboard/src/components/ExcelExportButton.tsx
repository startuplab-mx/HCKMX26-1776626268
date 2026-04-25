import { useState } from "react";

interface Props {
  /** Excel-friendly XML payload */
  buildXml: () => string;
  /** Filename without extension; date suffix will be appended */
  filenamePrefix: string;
  label?: string;
}

const COLORS = {
  idle: { bg: "rgba(76,175,80,0.12)", border: "rgba(76,175,80,0.3)", color: "#4CAF50" },
  exporting: { bg: "rgba(255,193,7,0.12)", border: "rgba(255,193,7,0.3)", color: "#FFC107" },
  done: { bg: "rgba(76,175,80,0.2)", border: "rgba(76,175,80,0.5)", color: "#4CAF50" },
};

const escapeXml = (s: unknown) =>
  String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** Wrap header + rows into Excel-compatible XML Spreadsheet 2003. */
export function buildExcelXml(
  sheetName: string,
  headers: string[],
  rows: (string | number)[][]
): string {
  const cell = (v: string | number, type: "String" | "Number" = "String") =>
    `<Cell><Data ss:Type="${type}">${escapeXml(v)}</Data></Cell>`;
  return `<?xml version="1.0" encoding="UTF-8"?>
<?mso-application progid="Excel.Sheet"?>
<Workbook xmlns="urn:schemas-microsoft-com:office:spreadsheet"
 xmlns:ss="urn:schemas-microsoft-com:office:spreadsheet">
 <Worksheet ss:Name="${escapeXml(sheetName)}">
  <Table>
   <Row>${headers.map((h) => cell(h)).join("")}</Row>
${rows
  .map(
    (row) =>
      `   <Row>${row
        .map((c) => cell(c, typeof c === "number" ? "Number" : "String"))
        .join("")}</Row>`
  )
  .join("\n")}
  </Table>
 </Worksheet>
</Workbook>`;
}

export function ExcelExportButton({ buildXml, filenamePrefix, label = "EXPORTAR XLS" }: Props) {
  const [status, setStatus] = useState<"idle" | "exporting" | "done">("idle");

  const handleExport = () => {
    setStatus("exporting");
    setTimeout(() => {
      const xml = buildXml();
      const blob = new Blob([xml], {
        type: "application/vnd.ms-excel;charset=utf-8",
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${filenamePrefix}_${new Date().toISOString().slice(0, 10)}.xls`;
      a.click();
      URL.revokeObjectURL(url);
      setStatus("done");
      setTimeout(() => setStatus("idle"), 2000);
    }, 400);
  };

  const c = COLORS[status];

  return (
    <button
      onClick={handleExport}
      disabled={status === "exporting"}
      style={{
        display: "flex",
        alignItems: "center",
        gap: "6px",
        padding: "5px 12px",
        borderRadius: "6px",
        border: `1px solid ${c.border}`,
        background: c.bg,
        color: c.color,
        fontSize: "11px",
        fontFamily: "Space Grotesk",
        fontWeight: 600,
        cursor: status === "exporting" ? "wait" : "pointer",
        transition: "all 0.2s",
        letterSpacing: "0.04em",
      }}
    >
      {status === "exporting" ? (
        <>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
            <line x1="12" y1="2" x2="12" y2="6" />
            <line x1="12" y1="18" x2="12" y2="22" />
            <line x1="4.93" y1="4.93" x2="7.76" y2="7.76" />
            <line x1="16.24" y1="16.24" x2="19.07" y2="19.07" />
            <line x1="2" y1="12" x2="6" y2="12" />
            <line x1="18" y1="12" x2="22" y2="12" />
          </svg>
          GENERANDO…
        </>
      ) : status === "done" ? (
        <>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="20 6 9 17 4 12" />
          </svg>
          DESCARGADO
        </>
      ) : (
        <>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
            <polyline points="7 10 12 15 17 10" />
            <line x1="12" y1="15" x2="12" y2="3" />
          </svg>
          {label}
        </>
      )}
    </button>
  );
}
