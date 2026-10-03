import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";
import * as stylex from "@stylexjs/stylex";
import { ButtonLink, Button, Input, PageHeader } from "../components/ui";
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
import { ConfirmDialog } from "../components/ConfirmDialog";
import { RecurringInvoiceTemplateModal } from "../components/RecurringInvoiceTemplateModal";
import { useParams } from "react-router-dom";
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
  weekly: "ugentligt",
  monthly: "månedligt",
  quarterly: "kvartalsvist",
  yearly: "årligt",
};
const CHANNEL_LABELS: Record<"manual" | "email" | "digisense", string> = {
  manual: "manuel kladde",
  email: "e-mail",
  digisense: "e-faktura",
};

export function RecurringInvoicesView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  // #386: the create-template modal is rendered into the view; it is toggled
  // by both the page-head primary button and the empty-state CTA.
  const [createOpen, setCreateOpen] = useState(false);
  const state = useAsync<Page>(
    async (signal) => {
      const [recurringInvoices, fiscalYears] = await Promise.all([
        api.recurringInvoices(slug, { signal }),
        api.fiscalYears(slug, { signal }),
      ]);
      return { recurringInvoices, fiscalYears };
    },
    [slug],
  );

  if (state.loading && !state.data)
    return <Loading label="Henter skabeloner…" />;
  if (state.error && !state.data)
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
    <section className="statement" data-cockpit-page="invoice-templates" data-evidence-issue="655">
      {state.error && <div className="banner warning" role="alert">Status kunne ikke opdateres. Din formular er bevaret; oplysningerne bag den er fra den seneste gennemførte læsning.</div>}
      <PageHeader title="Faktura-skabeloner" actions={<><div className={["row-actions", stylex.props(viewStyles.site0).className].filter(Boolean).join(" ")} >
          {!selectedYearArchived && (
            <Button requiredPermission="company.draft.write"
              type="button"
              className="btn"
              onClick={() => setCreateOpen(true)}
            >
              Opret skabelon
            </Button>
          )}
          <ButtonLink className="btn secondary" to={`/companies/${slug}/fakturaer`}>
            Tilbage til fakturaer
          </ButtonLink>
        </div></>}>
        <div>

          <p className="muted">
            Gentagne fakturaer — den næste i hver række kan udstedes med ét
            klik. Generering er idempotent: et nyt klik på samme periode
            udsteder ikke en ny faktura.
          </p>
        </div>

      </PageHeader>

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
            <Button requiredPermission="company.draft.write"
              type="button"
              className="btn"
              onClick={() => setCreateOpen(true)}
            >
              Opret skabelon
            </Button>
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
  const [savedAsOfDate, setSavedAsOfDate] = useState(template.nextIssueDate);
  const [busy, setBusy] = useState(false);
  const [retireBusy, setRetireBusy] = useState(false);
  const [retiring, setRetiring] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const outcome = useMutationOutcome(onReload);
  const markDateSaved = useUnsavedChanges(asOfDate !== savedAsOfDate);
  async function generate() {
    if (outcome.isBlocked()) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const result = await api.generateRecurringInvoice(
        slug,
        template.id,
        asOfDate,
      ).catch(outcome.reject);
      if (result.created) {
        setNotice(
          `Udstedte faktura ${result.invoiceNumber ?? ""} for ${result.issueDate ?? asOfDate}.`,
        );
      } else {
        setNotice(
          `Eksisterende faktura ${result.invoiceNumber ?? ""} blev returneret — perioden var allerede genereret.`,
        );
      }
      setSavedAsOfDate(asOfDate); markDateSaved(); onReload();
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
  async function retire(reason: string) {
    if (outcome.isBlocked()) return;
    setRetireBusy(true);
    setError(null);
    setNotice(null);
    try {
      await api.retireRecurringInvoiceTemplate(
        slug,
        template.id,
        reason && reason.trim().length > 0 ? reason.trim() : undefined,
      ).catch(outcome.reject);
      setNotice(`Skabelonen "${template.name}" er deaktiveret.`);
      setRetiring(false);
      onReload();
    } catch (err) {
      throw err;
    } finally {
      setRetireBusy(false);
    }
  }

  return (
    <div className={["card", stylex.props(viewStyles.site1).className].filter(Boolean).join(" ")} >
    {outcome.feedback}
      <h4 {...stylex.props(viewStyles.site2)}>
        {template.name}{" "}
        {!template.active && <span className="muted">(tilbagetrukken)</span>}
      </h4>
      <p className="muted">
        {(template.intervalCount ?? 1) > 1 ? `hver ${template.intervalCount}. ` : ""}{INTERVAL_LABELS[template.interval]} · {CHANNEL_LABELS[template.deliveryChannel ?? "manual"]} · næste udstedelse{" "}
        {template.nextIssueDate} · betalingsfrist {template.paymentTermsDays}{" "}
        dage
        {template.notes ? ` · ${template.notes}` : ""}
      </p>

      {template.active && (
        <div className={["row-actions", stylex.props(viewStyles.site3).className].filter(Boolean).join(" ")} >
          <label>
            Udsted som af
            <Input
              type="date"
              value={asOfDate}
              onChange={(e) => setAsOfDate(e.target.value)}
              disabled={outcome.blocked || (busy || retireBusy)}
            />
          </label>
          <Button requiredPermission="company.draft.write"
            className="btn"
            onClick={generate}
            disabled={outcome.blocked || (busy || retireBusy || asOfDate.length !== 10)}
            type="button"
          >
            {busy ? "Genererer…" : "Generér"}
          </Button>
          <Button requiredPermission="company.draft.write" variant="secondary"
            className="btn secondary"
            onClick={() => setRetiring(true)}
            disabled={outcome.blocked || busy || retireBusy}
            type="button"
            aria-label={`Deaktivér skabelonen ${template.name}`}
          >
            {retireBusy ? "Deaktiverer…" : "Deaktivér"}
          </Button>
        </div>
      )}

      {!template.active && (
        <p className={["muted", stylex.props(viewStyles.site4).className].filter(Boolean).join(" ")} >
          Skabelonen er deaktiveret og kan ikke længere generere fakturaer.
          Tidligere genererede fakturaer (nedenfor) er bevaret uændret.
        </p>
      )}

      {retiring && <ConfirmDialog
        title={`Deaktivér skabelonen ${template.name}?`}
        body={<p>Skabelonen kan ikke genaktiveres og kan ikke generere flere fakturaer. Tidligere fakturaer og revisionshistorik bevares. Opret en ny skabelon, hvis beløb eller frekvens skal ændres.</p>}
        confirmLabel="Deaktivér skabelon"
        confirmKind="danger"
        noteLabel="Årsag (valgfri)"
        onConfirm={retire}
        onClose={() => setRetiring(false)} onRefresh={onReload}
      />}

      {error && <Banner kind="error">{error}</Banner>}
      {notice && <Banner kind="success">{notice}</Banner>}

      {template.generations.length > 0 && (
        <div className={["table-scroll", stylex.props(viewStyles.site5).className].filter(Boolean).join(" ")} >
          <table className="data statement-table responsive-table" aria-label="Udstedte fakturaer fra skabelonen">
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

const viewStyles = stylex.create({
site0: { gap: 8 },
site1: { marginBottom: 16 },
site2: { marginTop: 0 },
site3: { alignItems: "center", gap: 12 },
site4: { fontStyle: "italic" },
site5: { marginTop: 12 }
});
