// Faktura-skabeloner — the cockpit surface for recurring-invoice templates.
//
// The deterministic core (createRecurringInvoiceTemplate / generateRecurringInvoice)
// is already in place — this view lists the templates, surfaces their next-issue
// date, and lets a human generate the next invoice with one click. Generation
// is idempotent, so re-clicking is safe.
//
// Templates are currently created via the CLI (`rentemester recurring-invoice
// create`); the in-cockpit creation form is a follow-up.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type {
  CompanyRecurringInvoices,
  FiscalYearEntry,
  RecurringInvoiceTemplateRow,
} from "../lib/types";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";

type Page = {
  recurringInvoices: CompanyRecurringInvoices;
  fiscalYears: FiscalYearEntry[];
};

const INTERVAL_LABELS: Record<RecurringInvoiceTemplateRow["interval"], string> = {
  monthly: "månedligt",
  quarterly: "kvartalsvist",
  yearly: "årligt",
};

export function RecurringInvoicesView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<Page>(
    async () => {
      const [recurringInvoices, fiscalYears] = await Promise.all([
        api.recurringInvoices(slug),
        api.fiscalYears(slug),
      ]);
      return { recurringInvoices, fiscalYears };
    },
    [slug],
  );

  if (state.loading && !state.data)
    return <Loading label="Henter skabeloner…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const { recurringInvoices: r, fiscalYears } = state.data!;
  const selectedYear =
    year ??
    fiscalYears.find((y) => y.source === "live")?.label ??
    fiscalYears[0]?.label ??
    String(new Date().getFullYear());
  const active = r.templates.filter((t) => t.active);
  const retired = r.templates.filter((t) => !t.active);

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>Faktura-skabeloner</h2>
          <p className="muted">
            Gentagne fakturaer — den næste i hver række kan udstedes med ét
            klik. Generering er idempotent: et nyt klik på samme periode
            udsteder ikke en ny faktura.
          </p>
        </div>
        <Link className="btn secondary" to={`/companies/${slug}/fakturaer`}>
          Tilbage til fakturaer
        </Link>
      </div>

      <CompanyNav
        slug={slug}
        years={fiscalYears}
        selectedYear={selectedYear}
        onYearChange={setYear}
      />

      {r.templates.length === 0 ? (
        <div className="card archived-notice">
          <h3>Ingen skabeloner endnu</h3>
          <p className="muted">
            Der er ikke oprettet nogen faktura-skabeloner for denne
            virksomhed. Når du har en gentagen faktura — fx et månedligt
            abonnement eller en kvartalsvis ydelse — kan du oprette en
            skabelon her, og cockpittet udsteder den næste faktura med ét
            klik.
          </p>
          <p className="muted">
            Oprettelse direkte fra cockpittet er på vej. Indtil da kan du
            oprette en almindelig faktura under{" "}
            <Link to={`/companies/${slug}/fakturaer`}>Fakturaer</Link>.
          </p>
        </div>
      ) : (
        <>
          {active.length > 0 && (
            <div className="section">
              <h3>Aktive ({active.length})</h3>
              {active.map((t) => (
                <TemplateCard
                  key={t.id}
                  template={t}
                  slug={slug}
                  onReload={state.reload}
                />
              ))}
            </div>
          )}
          {retired.length > 0 && (
            <div className="section">
              <h3>Tilbagetrukne ({retired.length})</h3>
              {retired.map((t) => (
                <TemplateCard
                  key={t.id}
                  template={t}
                  slug={slug}
                  onReload={state.reload}
                />
              ))}
            </div>
          )}
        </>
      )}
    </section>
  );
}

/** One template's card — header, generate action, and its past generations. */
function TemplateCard({
  template,
  slug,
  onReload,
}: {
  template: RecurringInvoiceTemplateRow;
  slug: string;
  onReload: () => void;
}) {
  const [asOfDate, setAsOfDate] = useState(template.nextIssueDate);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  async function generate() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const result = await api.generateRecurringInvoice(
        slug,
        template.id,
        asOfDate,
      );
      if (result.created) {
        setNotice(
          `Udstedte faktura ${result.invoiceNumber ?? ""} for ${result.issueDate ?? asOfDate}.`,
        );
      } else {
        setNotice(
          `Eksisterende faktura ${result.invoiceNumber ?? ""} blev returneret — perioden var allerede genereret.`,
        );
      }
      onReload();
    } catch (err) {
      const e = err as { message?: string };
      setError(e?.message ?? "Genereringen kunne ikke gennemføres.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="card" style={{ marginBottom: 16 }}>
      <h4 style={{ marginTop: 0 }}>
        {template.name}{" "}
        {!template.active && <span className="muted">(tilbagetrukken)</span>}
      </h4>
      <p className="muted">
        {INTERVAL_LABELS[template.interval]} · næste udstedelse{" "}
        {template.nextIssueDate} · betalingsfrist {template.paymentTermsDays}{" "}
        dage
        {template.notes ? ` · ${template.notes}` : ""}
      </p>

      {template.active && (
        <div className="row-actions" style={{ alignItems: "center", gap: 12 }}>
          <label>
            Udsted som af
            <input
              type="date"
              value={asOfDate}
              onChange={(e) => setAsOfDate(e.target.value)}
              disabled={busy}
            />
          </label>
          <button
            className="btn"
            onClick={generate}
            disabled={busy || asOfDate.length !== 10}
            type="button"
          >
            {busy ? "Genererer…" : "Generér"}
          </button>
        </div>
      )}

      {error && <Banner kind="error">{error}</Banner>}
      {notice && <Banner kind="success">{notice}</Banner>}

      {template.generations.length > 0 && (
        <div className="table-scroll" style={{ marginTop: 12 }}>
          <table className="data statement-table">
            <thead>
              <tr>
                <th>Periode</th>
                <th>Fakturanr.</th>
                <th>Udstedt</th>
                <th>Leveringsperiode</th>
              </tr>
            </thead>
            <tbody>
              {template.generations.map((g) => (
                <tr key={g.id}>
                  <td className="account-no">#{g.periodIndex}</td>
                  <td className="account-no">{g.invoiceNumber}</td>
                  <td className="entry-date">{g.issueDate}</td>
                  <td>
                    {g.deliveryPeriodStart && g.deliveryPeriodEnd
                      ? `${g.deliveryPeriodStart} → ${g.deliveryPeriodEnd}`
                      : "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
