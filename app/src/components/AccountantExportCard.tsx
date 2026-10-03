import { Button, Input } from "./ui";
// Revisor-eksport — the accountant-handoff package card.
//
// Generates the same .tar export the CLI's `system export-accountant` produces,
// packed by the server and streamed back as a single .tar download. The owner
// uses this once a year (or per quarter) to hand the bookkeeping over to the
// revisor.
//
// #373 — Extracted out of `ManageCompanyView` so the card can also live on
// Overblik (Dashboard), where year-end "afslut periode"-actions naturally
// belong. The Administrér placement is preserved unchanged; this is purely
// about making the action discoverable.

import { useState } from "react";
import { api } from "../lib/api";
import { todayIso } from "../lib/format";
import { Banner } from "./Feedback";
import { useMutationOutcome } from "../lib/useMutationOutcome";

/**
 * Revisor-eksport — generates the accountant-handoff package and triggers a
 * single .tar download. Calls the same core export the CLI's
 * `system export-accountant` uses, packed by the server and streamed back.
 *
 * The default period spans the current calendar year up to today, which
 * matches the year-end use case. The owner can narrow or widen it to any
 * sub-period (e.g. one registered VAT period) before generating.
 */
export function AccountantExportCard({ slug }: { slug: string }) {
  const outcome = useMutationOutcome(undefined, "accountant-export", `company:${slug}`);
  // Use the LOCAL date — `toISOString()` is UTC and is off-by-one in Danish
  // evening hours (UTC+1/+2), defaulting the export period to tomorrow.
  const today = todayIso();
  const yearStart = `${today.slice(0, 4)}-01-01`;
  const [periodStart, setPeriodStart] = useState(yearStart);
  const [periodEnd, setPeriodEnd] = useState(today);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<{
    filename: string;
    journalEntryCount: number;
    documentCount: number;
    bankTransactionCount: number;
  } | null>(null);

  async function generate() {
    if (busy || outcome.isBlocked()) return;
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      const res = await api.accountantExport(slug, { periodStart, periodEnd }).catch(outcome.reject);
      // Trigger a browser download from the blob — the response is the only
      // copy of the package that leaves the server.
      const url = URL.createObjectURL(res.blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = res.filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      setDone({
        filename: res.filename,
        journalEntryCount: res.journalEntryCount,
        documentCount: res.documentCount,
        bankTransactionCount: res.bankTransactionCount,
      });
    } catch (err) {
      const e = err as { message?: string };
      setError(e?.message ?? "Eksporten kunne ikke gennemføres.");
    } finally {
      setBusy(false);
    }
  }

  const disabled =
    busy ||
    outcome.blocked ||
    periodStart.length !== 10 ||
    periodEnd.length !== 10 ||
    periodStart > periodEnd;

  return (
    <div className="card accountant-export">
      {outcome.feedback}
      <h3 className="section-title">Revisor-eksport</h3>
      <p className="muted">
        Pakker journal, bilag, banktransaktioner og audit-log for perioden i én
        .tar-fil. Serveren danner pakken, når du klikker «Generér og download».
        Du vælger selv, hvordan du deler den med din revisor.
      </p>
      <label>
        Fra
        <Input
          type="date"
          value={periodStart}
          onChange={(e) => setPeriodStart(e.target.value)}
          disabled={busy}
        />
      </label>
      <label>
        Til
        <Input
          type="date"
          value={periodEnd}
          onChange={(e) => setPeriodEnd(e.target.value)}
          disabled={busy}
        />
      </label>
      {error && <Banner kind="error">{error}</Banner>}
      {done && (
        <Banner kind="success">
          Hentede {done.filename} — {done.journalEntryCount} posteringer,{" "}
          {done.documentCount} bilag, {done.bankTransactionCount}{" "}
          banktransaktioner.
        </Banner>
      )}
      <div className="row-actions">
        <Button
          requiredPermission="company.export"
          className="btn"
          onClick={generate}
          disabled={disabled}
          type="button"
        >
          {busy ? "Genererer…" : "Generér og download"}
        </Button>
      </div>
    </div>
  );
}
