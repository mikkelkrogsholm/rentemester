// Faktura-skabeloner — the cockpit surface for recurring-invoice templates.
//
// The deterministic core (createRecurringInvoiceTemplate / generateRecurringInvoice
// / retireRecurringInvoiceTemplate) is already in place — this view lists the
// templates, surfaces their next-issue date, lets a human generate the next
// invoice with one click, lets the owner retire a template that should no
// longer suggest itself (#435), and — as of #386 — lets the owner create a
// new template from the cockpit instead of having to use the CLI. Generation
// is idempotent, so re-clicking is safe.
//
// Templates are append-only by schema: a retired template cannot be
// reactivated, and identity/payload columns cannot be mutated. When an owner
// needs to change terms (price, frequency, customer), they retire the old
// template and create a new one — past generations stay on the original
// template's history.

import { useState } from "react";
import { RecurringInvoiceTemplateModal } from "../components/RecurringInvoiceTemplateModal";
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
  // #386: the create-template modal is rendered into the view; it is toggled
  // by both the page-head primary button and the empty-state CTA.
  const [createOpen, setCreateOpen] = useState(false);
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
  // #386: the selected fiscal year decides whether the create button is
  // shown. Archived years are read-only across the cockpit (mirrors
  // InvoicesView, BankView etc.), so an archived year hides the CTA without
  // removing the read-only listing of past templates.
  const selectedYearArchived =
    fiscalYears.find((y) => y.label === selectedYear)?.source === "archive";

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
        <div className="row-actions" style={{ gap: 8 }}>
          {!selectedYearArchived && (
            <button
              type="button"
              className="btn"
              onClick={() => setCreateOpen(true)}
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

      {r.templates.length === 0 ? (
        <div className="card archived-notice">
          <h3>Ingen skabeloner endnu</h3>
          <p className="muted">
            Der er ikke oprettet nogen faktura-skabeloner for denne
            virksomhed. Når du har en gentagen faktura — fx et månedligt
            abonnement eller en kvartalsvis ydelse — opretter du en skabelon,
            og cockpittet udsteder den næste faktura med ét klik.
          </p>
          {selectedYearArchived ? (
            <p className="muted">
              Regnskabsåret er arkiveret. Skift til et aktivt år for at oprette
              en skabelon.
            </p>
          ) : (
            <button
              type="button"
              className="btn"
              onClick={() => setCreateOpen(true)}
            >
              Opret skabelon
            </button>
          )}
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

      {createOpen && (
        <RecurringInvoiceTemplateModal
          slug={slug}
          onCreated={state.reload}
          onClose={() => setCreateOpen(false)}
        />
      )}
    </section>
  );
}

/** One template's card — header, generate action, retire action, and history. */
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
  const [retireBusy, setRetireBusy] = useState(false);
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

  /**
   * Retire (deactivate) the template. Templates are append-only by schema:
   * once retired they cannot be reactivated and identity/payload columns
   * cannot be mutated. To change terms, the owner creates a new template
   * — historical generations on the old template are preserved untouched.
   */
  async function retire() {
    // eslint-disable-next-line no-alert
    const confirmed = window.confirm(
      `Deaktivér skabelonen "${template.name}"?\n\n` +
        "En deaktiveret skabelon kan ikke generere flere fakturaer og kan ikke " +
        "genaktiveres (skabeloner er append-only). Tidligere genererede fakturaer " +
        "ændres ikke. Hvis kunden bare har ændret beløb/frekvens: deaktivér her " +
        "og opret en ny skabelon med de rette vilkår.",
    );
    if (!confirmed) return;
    setRetireBusy(true);
    setError(null);
    setNotice(null);
    try {
      // eslint-disable-next-line no-alert
      const reason =
        window.prompt(
          "Kort årsag (valgfri — vises i revisionssporet):",
          "",
        ) ?? undefined;
      await api.retireRecurringInvoiceTemplate(
        slug,
        template.id,
        reason && reason.trim().length > 0 ? reason.trim() : undefined,
      );
      setNotice(`Skabelonen "${template.name}" er deaktiveret.`);
      onReload();
    } catch (err) {
      const e = err as { message?: string };
      setError(e?.message ?? "Skabelonen kunne ikke deaktiveres.");
    } finally {
      setRetireBusy(false);
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
              disabled={busy || retireBusy}
            />
          </label>
          <button
            className="btn"
            onClick={generate}
            disabled={busy || retireBusy || asOfDate.length !== 10}
            type="button"
          >
            {busy ? "Genererer…" : "Generér"}
          </button>
          <button
            className="btn secondary"
            onClick={retire}
            disabled={busy || retireBusy}
            type="button"
            aria-label={`Deaktivér skabelonen ${template.name}`}
          >
            {retireBusy ? "Deaktiverer…" : "Deaktivér"}
          </button>
        </div>
      )}

      {!template.active && (
        <p className="muted" style={{ fontStyle: "italic" }}>
          Skabelonen er deaktiveret og kan ikke længere generere fakturaer.
          Tidligere genererede fakturaer (nedenfor) er bevaret uændret.
        </p>
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
