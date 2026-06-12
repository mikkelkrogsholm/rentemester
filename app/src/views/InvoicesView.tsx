// Fakturaer — the per-company issued invoices (cockpit-redesign iteration 5;
// human write-actions added in #213, slice 4).
//
// Renders `/api/companies/:slug/invoices?year=`: the sales invoices issued in
// the selected fiscal year, each with its settlement status (kladde / bogført
// / betalt / forfalden …). Summary cards above the table give the year's gross
// total, the outstanding total and the overdue count. A company with no issued
// invoices shows a graceful empty state. All money fields are kroner —
// `formatKroner` is used throughout.
//
// Slice 4 makes the view write-capable for the human-mode invoice actions:
//   - "Udsted faktura" (page action) opens the multi-line InvoiceIssueModal;
//   - per row, "Afstem" settles an issued invoice against a bank payment via
//     a ConfirmDialog because the posting is write-irreversible.
// Every write action is hidden for an archived (read-only) year.
//
// Issue #385: a per-row "Bogfør" action used to live here too. Every row in
// this list is already posted (the `InvoiceStatus` union has no "draft" and
// the empty state copy reads "Udstedte fakturaer vises her, så snart de er
// bogført"), so re-offering "Bogfør" only tempted the owner into a
// double-post. The action was removed from the cockpit; ledger reposting
// remains available via `invoice post` in the CLI for the rare repair case.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner, todayIso } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type {
  CompanyInvoiceRow,
  CompanyInvoices,
  InvoiceStatus,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { InvoiceIssueModal } from "../components/InvoiceIssueModal";

/** Human label + flag tone for each settlement status. */
const STATUS_META: Record<
  InvoiceStatus,
  { label: string; tone: "ok" | "warning" | "critical" | "neutral" }
> = {
  open: { label: "Bogført", tone: "neutral" },
  paid: { label: "Betalt", tone: "ok" },
  credited: { label: "Krediteret", tone: "warning" },
  refunded: { label: "Refunderet", tone: "warning" },
  overpaid: { label: "Overbetalt", tone: "warning" },
  written_off: { label: "Afskrevet", tone: "critical" },
  overdue: { label: "Forfalden", tone: "critical" },
};

export function InvoicesView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyInvoices>(
    () => api.invoices(slug, year),
    [slug, year],
  );
  // True while the invoice-issue modal (#213, slice 4) is open.
  const [issuing, setIssuing] = useState(false);
  // The invoice row whose "Afstem" ConfirmDialog is open, if any.
  const [settling, setSettling] = useState<CompanyInvoiceRow | null>(null);
  // The invoice row whose "Krediter" ConfirmDialog is open, if any (#412).
  const [crediting, setCrediting] = useState<CompanyInvoiceRow | null>(null);
  // The invoice row whose "Send som e-faktura" ConfirmDialog is open (#428).
  const [sendingPublic, setSendingPublic] = useState<CompanyInvoiceRow | null>(null);
  // The invoice row whose "Send på mail" ConfirmDialog is open (#429).
  const [sendingEmail, setSendingEmail] = useState<CompanyInvoiceRow | null>(null);
  // The invoice row whose "Send rykker" ConfirmDialog is open (#434).
  const [sendingReminder, setSendingReminder] = useState<CompanyInvoiceRow | null>(null);
  // Whether the rykker dialog's "Bogfør rykkergebyr nu"-checkbox is on (#434).
  // Default ON because the typical SMB owner WANTS the fee booked — it's the
  // whole point of having a registered reminder for the legal trail.
  const [reminderBookFee, setReminderBookFee] = useState(true);

  if (state.loading && !state.data)
    return <Loading label="Henter fakturaer…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const inv = state.data!;
  const currency = inv.company.currency || "DKK";

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{inv.company.name}</h2>
          <p className="muted">
            {inv.company.cvr ? `CVR ${inv.company.cvr} · ` : ""}
            {inv.company.country} · {currency} · Fakturaer
          </p>
        </div>
        <div className="row-actions">
          {/* The issue write action — hidden for an archived (read-only) year,
              where no live ledger is available to issue into. */}
          {!inv.archived && (
            <button
              type="button"
              className="btn"
              onClick={() => setIssuing(true)}
            >
              Udsted faktura
            </button>
          )}
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

      <CompanyNav
        slug={slug}
        years={inv.fiscalYears}
        selectedYear={inv.selectedYear}
        onYearChange={setYear}
      />

      {issuing && (
        <InvoiceIssueModal
          slug={slug}
          onIssued={state.reload}
          onClose={() => setIssuing(false)}
        />
      )}

      {settling && (
        <ConfirmDialog
          title="Afstem faktura mod bankbetaling"
          body={
            <p>
              Afstem faktura <strong>{settling.invoiceNo}</strong> mod en
              indgående bankbetaling med samme reference. Afstemningen lægger en
              postering og kan ikke fortrydes.
            </p>
          }
          confirmLabel="Afstem faktura"
          confirmKind="danger"
          noteLabel="Bankreference"
          notePlaceholder="Referencen på banktransaktionen"
          onConfirm={async (reference) => {
            if (!reference.trim()) {
              throw {
                code: "bad_request",
                message: "Angiv referencen på bankbetalingen.",
              };
            }
            await api.settleInvoice(slug, {
              invoiceDocumentId: settling.documentId,
              bankTransactionReference: reference.trim(),
            });
            state.reload();
          }}
          onClose={() => setSettling(null)}
        />
      )}

      {/* #412: the Krediter ConfirmDialog. A credit note appends a reversing
          journal entry (and a new credit-note document), so the action is
          write-irreversible. A begrundelse is required for the audit trail —
          a blank value blocks the call before it reaches the server. */}
      {crediting && (
        <ConfirmDialog
          title="Udsted kreditnota"
          body={
            <p>
              Kreditér faktura <strong>{crediting.invoiceNo}</strong>. En
              kreditnota bogføres som modgående postering med eget nummer fra
              kreditnota-serien. Handlingen kan ikke fortrydes og kræver en
              begrundelse til revisionssporet.
            </p>
          }
          confirmLabel="Udsted kreditnota"
          confirmKind="danger"
          noteLabel="Begrundelse"
          notePlaceholder="Hvorfor krediteres fakturaen?"
          onConfirm={async (reason) => {
            if (!reason.trim()) {
              throw {
                code: "bad_request",
                message:
                  "Angiv en begrundelse for kreditnotaen — den indgår i revisionssporet.",
              };
            }
            await api.creditInvoice(slug, {
              invoiceDocumentId: crediting.documentId,
              // Use the LOCAL date — `toISOString()` is UTC and would mis-date
              // the BOOKED credit note in Danish evening hours (UTC+1/+2).
              issueDate: todayIso(),
              reason: reason.trim(),
            });
            state.reload();
          }}
          onClose={() => setCrediting(null)}
        />
      )}

      {/* #428: Send som e-faktura ConfirmDialog. The action is only ever
          offered for rows with an EAN-number on a public-recipient buyer;
          the dialog shows that EAN + the kanal so the owner can sanity-check
          who and where the invoice will be transmitted to. Write-irreversible
          (it records a peppol_submissions row + an audit_log entry), so the
          server requires `confirm: true` — the dialog's primary button maps
          to that flag. */}
      {sendingPublic && (
        <ConfirmDialog
          title="Send faktura som e-faktura"
          body={
            <div>
              <p>
                Send faktura <strong>{sendingPublic.invoiceNo}</strong> til{" "}
                <strong>{sendingPublic.customerName ?? "modtageren"}</strong> som
                e-faktura via NemHandel/PEPPOL.
              </p>
              <dl className="confirm-meta">
                <div>
                  <dt>EAN-nummer</dt>
                  <dd>{sendingPublic.buyerEanNumber ?? "—"}</dd>
                </div>
                <div>
                  <dt>Kanal</dt>
                  <dd>NemHandel (PEPPOL)</dd>
                </div>
                <div>
                  <dt>Handling</dt>
                  <dd>Sendes nu</dd>
                </div>
              </dl>
              <p className="muted">
                Afsendelsen registreres i revisionssporet og kan ikke fortrydes.
              </p>
            </div>
          }
          confirmLabel="Send e-faktura"
          confirmKind="danger"
          onConfirm={async () => {
            await api.sendInvoiceAsEInvoice(slug, {
              invoiceDocumentId: sendingPublic.documentId,
            });
            state.reload();
          }}
          onClose={() => setSendingPublic(null)}
        />
      )}

      {/* #429: Send på mail ConfirmDialog. The action is only offered for
          rows where the customer has an e-mail on the kontaktkort, so the
          dialog can prefill the recipient. The recipient field is editable
          (noteLabel) so the owner can override the customer's default
          address. Write-irreversible (it appends an `email_send_log` row
          + an `audit_log` entry), so the server requires `confirm: true`.
          Replaces the missing CLI step `invoice send` — SMB owners no
          longer need to download the PDF and open their mail client to get
          the invoice out to the customer. */}
      {sendingEmail && (
        <ConfirmDialog
          title="Send faktura på mail"
          body={
            <div>
              <p>
                Send faktura <strong>{sendingEmail.invoiceNo}</strong> til{" "}
                <strong>{sendingEmail.customerName ?? "modtageren"}</strong>{" "}
                med PDF'en vedhæftet.
              </p>
              <dl className="confirm-meta">
                <div>
                  <dt>Emne</dt>
                  <dd>Faktura {sendingEmail.invoiceNo}</dd>
                </div>
                <div>
                  <dt>Vedhæftning</dt>
                  <dd>{sendingEmail.invoiceNo}.pdf</dd>
                </div>
              </dl>
              <p className="muted">
                Modtageren kan ændres herunder. Afsendelsen registreres i
                revisionssporet og kan ikke fortrydes.
              </p>
            </div>
          }
          confirmLabel="Send faktura"
          confirmKind="danger"
          noteLabel="Modtager"
          notePlaceholder="kunde@eksempel.dk"
          noteInitialValue={sendingEmail.customerEmail ?? ""}
          noteInputType="email"
          onConfirm={async (recipient) => {
            const trimmed = recipient.trim();
            if (!trimmed) {
              throw {
                code: "bad_request",
                message:
                  "Angiv modtagerens e-mailadresse — fakturaen kan ikke sendes uden.",
              };
            }
            await api.sendInvoiceByEmail(slug, {
              invoiceDocumentId: sendingEmail.documentId,
              to: trimmed,
            });
            state.reload();
          }}
          onClose={() => setSendingEmail(null)}
        />
      )}

      {/* #434 — Send rykker ConfirmDialog. Only ever rendered when the row's
          state allows it: overdue, has a customer e-mail, and fewer than 3
          reminders already registered. The body surfaces (a) days overdue,
          (b) the recipient's e-mail, (c) which reminder this is (1./2./3.),
          (d) the fee (100 kr — statutory cap per rentel. § 9b), and (e) a
          checkbox so the owner can opt out of the auto-booking of the fee.
          The recipient is editable so the owner can override the stored
          e-mail. Write-irreversible (registers a reminder, optionally
          appends a journal entry, always appends an `email_send_log` +
          `audit_log` row) — `confirm: true` is set by the API client. */}
      {sendingReminder && (() => {
        const nextSeq = (sendingReminder.lastReminderSequence ?? 0) + 1;
        const ord = nextSeq === 1 ? "1." : nextSeq === 2 ? "2." : "3.";
        return (
          <ConfirmDialog
            title="Send rykker til kunden"
            body={
              <div>
                <p>
                  Send <strong>{ord} rykker</strong> for faktura{" "}
                  <strong>{sendingReminder.invoiceNo}</strong> til{" "}
                  <strong>
                    {sendingReminder.customerName ?? "modtageren"}
                  </strong>
                  .
                </p>
                <dl className="confirm-meta">
                  <div>
                    <dt>Dage forfalden</dt>
                    <dd>{sendingReminder.overdueDays} dage</dd>
                  </div>
                  <div>
                    <dt>Modtager</dt>
                    <dd>{sendingReminder.customerEmail ?? "—"}</dd>
                  </div>
                  <div>
                    <dt>Rykkernummer</dt>
                    <dd>{ord} rykker (af maks. 3)</dd>
                  </div>
                  <div>
                    <dt>Rykkergebyr</dt>
                    <dd>100,00 kr (rentel. § 9b)</dd>
                  </div>
                </dl>
                <label className="modal-checkbox">
                  <input
                    type="checkbox"
                    checked={reminderBookFee}
                    onChange={(e) => setReminderBookFee(e.target.checked)}
                  />{" "}
                  Bogfør rykkergebyr (100 kr) i ledgeren nu
                </label>
                <p className="muted">
                  Modtageren kan ændres herunder. Afsendelsen registreres i
                  revisionssporet og kan ikke fortrydes.
                </p>
              </div>
            }
            confirmLabel="Send rykker nu"
            confirmKind="danger"
            noteLabel="Modtager"
            notePlaceholder="kunde@eksempel.dk"
            noteInitialValue={sendingReminder.customerEmail ?? ""}
            noteInputType="email"
            onConfirm={async (recipient) => {
              const trimmed = recipient.trim();
              if (!trimmed) {
                throw {
                  code: "bad_request",
                  message:
                    "Angiv modtagerens e-mailadresse — rykkeren kan ikke sendes uden.",
                };
              }
              await api.sendInvoiceReminder(slug, {
                invoiceDocumentId: sendingReminder.documentId,
                to: trimmed,
                bookFee: reminderBookFee,
              });
              state.reload();
            }}
            onClose={() => {
              setSendingReminder(null);
              setReminderBookFee(true);
            }}
          />
        );
      })()}

      {inv.archived ? (
        <ArchivedNotice year={inv.selectedYear} />
      ) : inv.invoices.length === 0 ? (
        <div className="card archived-notice">
          <h3>Ingen fakturaer endnu</h3>
          <p className="muted">
            Der er ikke udstedt salgsfakturaer i regnskabsåret{" "}
            {inv.selectedYear}. Udstedte fakturaer vises her, så snart de er
            bogført. Brug <em>Udsted faktura</em> for at lave en ny.
          </p>
        </div>
      ) : (
        <>
          <div className="status-grid invoices-summary">
            <div className="card status-card">
              <h3>Faktureret i alt</h3>
              <div className="status-figure">
                {formatKroner(inv.totalGross, currency)}
              </div>
              <p className="muted status-note">
                {inv.invoices.length}{" "}
                {inv.invoices.length === 1 ? "faktura" : "fakturaer"} i{" "}
                {inv.selectedYear}
              </p>
            </div>
            <div className="card status-card">
              <h3>Udestående</h3>
              <div
                className={`status-figure${
                  inv.totalOpen > 0 ? " status-alert" : ""
                }`}
              >
                {formatKroner(inv.totalOpen, currency)}
              </div>
              <p className="muted status-note">
                {inv.overdueCount > 0
                  ? `${inv.overdueCount} forfalden${
                      inv.overdueCount === 1 ? "" : "e"
                    }`
                  : "Ingen forfaldne fakturaer"}
              </p>
            </div>
          </div>

          <div className="card statement-card table-scroll">
            <table className="data statement-table">
              <thead>
                <tr>
                  <th>Fakturanr.</th>
                  <th>Kunde</th>
                  <th>Dato</th>
                  <th>Forfald</th>
                  <th className="num">Beløb inkl. moms</th>
                  <th className="num">Udestående</th>
                  <th>Status</th>
                  <th>Handlinger</th>
                </tr>
              </thead>
              <tbody>
                {inv.invoices.map((row) => {
                  const meta = STATUS_META[row.status];
                  // Settlement only makes sense while a balance is open.
                  const canSettle = row.openBalance > 0;
                  // #412: Krediter is offered for any posted invoice that has
                  // not already been written off / refunded / fully credited.
                  // A partial credit reduces the open balance but leaves the
                  // source invoice in its open/paid/overdue state, so those
                  // remain creditable until the core refuses on "already fully
                  // credited" (mapped to a 409 by the mutation pipeline).
                  const canCredit =
                    row.status !== "credited" &&
                    row.status !== "refunded" &&
                    row.status !== "written_off";
                  // #428: "Send som e-faktura" appears only when the buyer
                  // is a public recipient with a valid EAN-number (the
                  // server-side requirement for a NemHandel/PEPPOL send).
                  // Hidden once the invoice has been acknowledged by the
                  // access point — re-sending an already-acknowledged
                  // invoice would only confuse the owner. A `prepared`
                  // status (envelope recorded but not acknowledged) still
                  // allows a retry.
                  const canSendPublic =
                    Boolean(row.buyerEanNumber) &&
                    row.buyerPublicRecipient &&
                    row.peppolStatus?.status !== "acknowledged";
                  // #429: "Send på mail" appears only when the customer
                  // has an e-mail on the kontaktkort — without it the
                  // dialog has no recipient to prefill, and the issue
                  // body asks the cockpit to hide the action instead of
                  // surfacing an empty form.
                  const canSendEmail = Boolean(row.customerEmail);
                  // #434: "Send rykker" appears only when the row is
                  // overdue AND a customer e-mail is on file AND the
                  // statutory cap of 3 reminders has not been reached
                  // (rentel. § 9b). Hidden for archived years (no live
                  // ledger to register the reminder into).
                  const reminderSeq = row.lastReminderSequence ?? 0;
                  const canSendReminder =
                    row.status === "overdue" &&
                    Boolean(row.customerEmail) &&
                    reminderSeq < 3;
                  return (
                    <tr key={row.documentId}>
                      <td className="account-no">{row.invoiceNo}</td>
                      <td>{row.customerName ?? "—"}</td>
                      <td className="entry-date">{row.invoiceDate ?? "—"}</td>
                      <td className="entry-date">
                        {row.effectiveDueDate ?? "—"}
                      </td>
                      <td className="num">
                        {formatKroner(row.grossAmount, currency)}
                      </td>
                      <td className="num">
                        {row.openBalance > 0
                          ? formatKroner(row.openBalance, currency)
                          : "—"}
                      </td>
                      <td>
                        <span className={`flag ${meta.tone}`}>
                          {meta.label}
                          {/* Singular/plural — "· 1 dage" is wrong Danish. */}
                          {row.status === "overdue" && row.overdueDays > 0
                            ? ` · ${row.overdueDays} ${
                                row.overdueDays === 1 ? "dag" : "dage"
                              }`
                            : ""}
                        </span>
                        {/* #429 — surface a "Sendt {dato}" flag once the
                            invoice has been emailed from the cockpit so the
                            owner can see at a glance whether the customer
                            already got it. The date is the ISO timestamp
                            sliced to YYYY-MM-DD — the audit row carries the
                            full timestamp, the row only needs the day. */}
                        {row.lastEmailedAt && (
                          <span
                            className="flag ok"
                            title={`Sendt på mail ${row.lastEmailedAt}`}
                          >
                            Sendt {row.lastEmailedAt.slice(0, 10)}
                          </span>
                        )}
                        {/* #434 — surface the reminder sequence + date once
                            a rykker has been sent, so the owner can see at
                            a glance hvor i rykkerforløbet han er (1., 2., 3.
                            rykker) without re-reading the audit log. */}
                        {row.lastReminderAt && row.lastReminderSequence > 0 && (
                          <span
                            className="flag warning"
                            title={`Rykker registreret ${row.lastReminderAt}`}
                          >
                            {row.lastReminderSequence}. rykker sendt{" "}
                            {row.lastReminderAt.slice(0, 10)}
                          </span>
                        )}
                        {/* #428 — surface e-faktura status next to settlement
                            status so the owner can see at a glance whether
                            the invoice has been transmitted to NemHandel. */}
                        {row.peppolStatus && (
                          <span
                            className={`flag ${
                              row.peppolStatus.status === "acknowledged"
                                ? "ok"
                                : "neutral"
                            }`}
                            title={
                              row.peppolStatus.acknowledgedAt
                                ? `Bekræftet ${row.peppolStatus.acknowledgedAt}`
                                : `Reference ${row.peppolStatus.submissionReference}`
                            }
                          >
                            {row.peppolStatus.status === "acknowledged"
                              ? "Sendt som e-faktura"
                              : "E-faktura forberedt"}
                          </span>
                        )}
                      </td>
                      <td>
                        <div className="row-actions">
                          {/* #378: the PDF link is the primary action — the
                              whole point of issuing an invoice is to send it
                              to the customer. `target="_blank"` so the browser
                              opens it inline without losing the table view. */}
                          <a
                            className="btn secondary"
                            href={api.invoicePdfUrl(slug, row.documentId)}
                            target="_blank"
                            rel="noopener"
                          >
                            Hent PDF
                          </a>
                          {canSettle && (
                            <button
                              type="button"
                              className="btn secondary"
                              onClick={() => setSettling(row)}
                            >
                              Afstem
                            </button>
                          )}
                          {/* #412: per-row Krediter button. The action is
                              hidden for an archived (read-only) year — every
                              write-action in this view is — and for rows
                              already credited/refunded/written off. */}
                          {!inv.archived && canCredit && (
                            <button
                              type="button"
                              className="btn secondary"
                              onClick={() => setCrediting(row)}
                            >
                              Kreditér
                            </button>
                          )}
                          {/* #428 — "Send som e-faktura" is shown ONLY when
                              the customer has an EAN-number on file (a public
                              buyer). Hidden for archived years and once the
                              invoice has been acknowledged by the access
                              point. The button replaces the missing CLI step
                              `invoice submit-public-peppol` so SMB owners who
                              invoice the public sector no longer need a
                              terminal to get paid. */}
                          {!inv.archived && canSendPublic && (
                            <button
                              type="button"
                              className="btn secondary"
                              onClick={() => setSendingPublic(row)}
                            >
                              Send som e-faktura
                            </button>
                          )}
                          {/* #429 — "Send på mail" is shown ONLY when the
                              customer has an e-mail on the kontaktkort.
                              Hidden for archived years (no live ledger).
                              The button replaces the missing CLI step
                              `invoice send` so SMB owners no longer have to
                              download the PDF and open their own mail
                              client to get the invoice out to the customer. */}
                          {!inv.archived && canSendEmail && (
                            <button
                              type="button"
                              className="btn secondary"
                              onClick={() => setSendingEmail(row)}
                            >
                              Send på mail
                            </button>
                          )}
                          {/* #434 — "Send rykker" is shown ONLY for overdue
                              rows where the customer has an e-mail AND the
                              statutory 3-reminder cap (rentel. § 9b) has
                              not been reached. Hidden for archived years
                              (no live ledger). One click opens the
                              ConfirmDialog with the recipient, days
                              overdue, reminder number and a fee-booking
                              checkbox. */}
                          {!inv.archived && canSendReminder && (
                            <button
                              type="button"
                              className="btn secondary"
                              onClick={() => setSendingReminder(row)}
                            >
                              Send rykker
                            </button>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </>
      )}
    </section>
  );
}

function ArchivedNotice({ year }: { year: string }) {
  return (
    <div className="card archived-notice">
      <h3>Fakturaer er ikke tilgængelige for {year}</h3>
      <p className="muted">
        {year} er et arkiveret regnskabsår. Udstedte fakturaer føres kun i den
        aktive ledger og vises derfor ikke for et arkiveret år.
        Resultatopgørelse, balance, saldobalance og posteringer for {year} er
        tilgængelige.
      </p>
    </div>
  );
}
