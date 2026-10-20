import { useState, type FormEvent } from "react";
import { api } from "../lib/api";
import { todayIso, formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import { Button, Field, Input } from "../components/ui";
import { Banner, ErrorState, Loading } from "../components/Feedback";

/** Explicit dated read: changing inputs never silently changes the shown period. */
export function OssReportPanel({ slug }: { slug: string }) {
  const today = todayIso();
  const quarterMonth = Math.floor((Number(today.slice(5, 7)) - 1) / 3) * 3 + 1;
  const [from, setFrom] = useState(`${today.slice(0, 4)}-${String(quarterMonth).padStart(2, "0")}-01`);
  const [to, setTo] = useState(today);
  const [period, setPeriod] = useState<{ from: string; to: string } | null>(null);
  const state = useAsync((signal) => period ? api.ossReport(slug, period.from, period.to, { signal }) : Promise.resolve(null), [slug, period?.from, period?.to]);
  const report = state.data;
  function show(event: FormEvent) {
    event.preventDefault();
    if (period?.from === from && period.to === to) state.reload();
    else setPeriod({ from, to });
  }
  return <section className="card statement-card" aria-label="OSS-rapport">
    <h2>OSS-salg til EU-forbrugere</h2>
    <p className="muted">Kun salgsgrundlag i DKK fra eksplicit klassificerede posteringer. Rapporten beregner ikke moms pr. land eller sats og indberetter ikke noget.</p>
    <form className="row-actions" onSubmit={show}>
      <Field label="OSS fra"><Input type="date" required value={from} onChange={(event) => setFrom(event.target.value)} /></Field>
      <Field label="OSS til"><Input type="date" required min={from} value={to} onChange={(event) => setTo(event.target.value)} /></Field>
      <Button type="submit">Vis OSS-rapport</Button>
    </form>
    {state.loading && period ? <Loading label="Henter OSS-rapport…" /> : state.error ? <ErrorState message={state.error} onRetry={state.reload} /> : report && <>
      <p>OSS-periode: {report.periodStart} – {report.periodEnd}</p>
      {report.warnings.map((warning, index) => <Banner kind="warning" key={index}>{warning}</Banner>)}
      {report.errors.map((error, index) => <Banner kind="error" key={index}>{error}</Banner>)}
      {report.ok && <dl><dt>OSS-salgsgrundlag</dt><dd>{formatKroner(report.ossConsumerSalesBase, "DKK")}</dd><dt>Antal posteringer</dt><dd>{report.entryCount}</dd></dl>}
    </>}
  </section>;
}
