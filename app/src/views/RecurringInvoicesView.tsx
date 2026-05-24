// Faktura-skabeloner — the cockpit surface for recurring-invoice templates.
//
// The deterministic core (createRecurringInvoiceTemplate / generateRecurringInvoice)
// is already in place — this view lists the templates, surfaces their next-issue
// date, and lets a human generate the next invoice with one click. Generation
// is idempotent, so re-clicking is safe.
//
// #386: creating a template is now a cockpit write-action ("Opret skabelon"
// in the page-head opens a real form), so the SMB owner never has to drop
// into the terminal for daily invoice work. The button is hidden on an
// archived (read-only) year, like the other write-actions in the cockpit.

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
import { RecurringInvoiceTemplateModal } from "../components/RecurringInvoiceTemplateModal";

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
  // #386: true while the "Opret skabelon" modal is open.
  const [creating, setCreating] = useState(false);
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
  // #386: the write-actions are hidden when the cockpit shows an archived
  // (read-only) year — same rule as Udsted faktura / Indlæs bilag. A workspace
  // with no live year at all is treated as read-only too.
  const hasLiveYear = fiscalYears.some((y) => y.source === "live");
  const selectedYearEntry = fiscalYears.find((y) => y.label === selectedYear);
  const isArchivedYear =
    !hasLiveYear || selectedYearEntry?.source === "archive";
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
        <div className="row-actions">
          {/* #386: the create write-action — hidden for an archived
              (read-only) year, where no live ledger is available to add a
              template into. */}
          {!isArchivedYear && (
            <button
              type="button"
              className="btn"
              onClick={() => setCreating(true)}
            >
              Opret skabelon
            </button>
          )}
          <Link className="btn secondary" to={`/companies/${slug}/fakturaer`}>
            Tilbage til fakturaer
          </Link>
        </div>
      </div>

      <CompanyNav
        slug={slug}
        years={fiscalYears}
        selectedYear={selectedYear}
        onYearChange={setYear}
      />

      {creating && (
        <RecurringInvoiceTemplateModal
          slug={slug}
          onCreated={state.reload}
          onClose={() => setCreating(false)}
        />
      )}

      {r.templates.length === 0 ? (
        <div className="card archived-notice">
          <h3>Ingen skabeloner endnu</h3>
          <p className="muted">
            {isArchivedYear
              ? "Der er ikke oprettet nogen faktura-skabeloner for denne virksomhed. Arkiverede regnskabsår er skrivebeskyttede, så vælg et levende år for at oprette en skabelon."
              : "Der er ikke oprettet nogen faktura-skabeloner endnu. Klik på “Opret skabelon” for at sætte en op — fx en månedlig faktura til en fast kunde."}
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
