import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { ImportedReceivablesPanel } from "../components/ImportedReceivablesPanel";
import { NativeInvoiceEmail } from "../components/NativeInvoiceEmail";
import { NativeInvoicePost } from "../components/NativeInvoicePost";
import { NativeReminderClaims } from "../components/NativeReminderClaims";
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

import { useEffect, useMemo, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { Amount, Button, ButtonLink, FilterBar, Input, PageHeader, Pagination, Select } from "../components/ui";
import { api, ApiError } from "../lib/api";
import { formatDateDa, formatKroner, parseDanishAmount, todayIso } from "../lib/format";
import type {
  CompanyInvoiceRow,
  CompanyInvoices,
  InvoiceStatus,
} from "../lib/types";
import { useAsync } from "../lib/useAsync";
import { useCapabilities } from "../lib/useCapabilities";
import { listPagination, listReturnTo, workflowTo } from "./workflow-navigation";

// #UI-16 — the statutory late-payment reminder fee (rentel. § 9b), in kroner.
// One named constant, rendered through `formatKroner`, so the two places that
// quote it can never drift (was a hardcoded "100,00 kr" vs "100 kr").
const REMINDER_FEE_KRONER = 100;

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

/** Never offer delivery for retained acceptance, ambiguity or legacy evidence. */
export function canSendPublicInvoice(row:CompanyInvoiceRow):boolean {
  const status=row.peppolStatus;
  return Boolean(row.buyerEanNumber)&&row.buyerPublicRecipient&&
    !status?.redeliveryBlocked&&!status?.transmissionId&&!status?.legacy&&
    (status===null||status.status==="retryable");
}
export function canObservePublicInvoice(row:CompanyInvoiceRow):boolean {
  return Boolean(row.peppolStatus?.transmissionId)&&!row.peppolStatus?.legacy&&
    (row.peppolStatus?.status==="queued"||row.peppolStatus?.status==="prepared");
}

export function InvoicesView({ detail = false }: { detail?: boolean } = {}) {
  const { slug = "", documentId } = useParams();
  const { can } = useCapabilities(slug);
  const [params, setParams] = useSearchParams();
  const [interactionScope, setInteractionScope] = useState<string | null>(null);
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyInvoices>(
    (signal) => api.invoices(slug, year, { signal }),
    [slug, year],
  );
	// The archive schedule is deliberately a second read: it has its own
	// source hashes and must never be silently mixed into issued-invoice totals.
	const imported = useAsync(
		(signal) =>
			api.importedReceivables(
				slug,
				`${year ?? new Date().getFullYear()}-12-31`,
				{ signal },
			),
		[slug, year],
	);

  const native=Boolean(state.data?.nativeSales);
  const publicConfig=useAsync(signal=>native?api.publicEInvoiceConfig(slug,{signal}):Promise.resolve(null),[slug,native]);
  const [registeringPublic,setRegisteringPublic]=useState(false);
  const [publicNotice,setPublicNotice]=useState<string|null>(null);
  async function refreshPublic() {
    // This read never submits or releases the confirmation dialog's durable lock.
    await api.invoices(slug,year);
    if(native) await api.publicEInvoiceConfig(slug);
    state.reload();publicConfig.reload();
  }
  // The invoice row whose "Afstem" ConfirmDialog is open, if any.
  const [reminderClaimsId,setReminderClaimsId]=useState<number|null>(null);
  const [settlingImported, setSettlingImported] = useState<import("../lib/types").ImportedReceivableRow | null>(null);
  const [settling, setSettling] = useState<CompanyInvoiceRow | null>(null);
  // The invoice row whose "Krediter" ConfirmDialog is open, if any (#412).
  const [crediting, setCrediting] = useState<CompanyInvoiceRow | null>(null);
  const [creditGross, setCreditGross] = useState("");
  const [refunding, setRefunding] = useState<CompanyInvoiceRow | null>(null);
  // The invoice row whose "Send e-faktura" ConfirmDialog is open (#428).
  const [sendingPublic, setSendingPublic] = useState<CompanyInvoiceRow | null>(null);
  const [checkingPublicStatus, setCheckingPublicStatus] = useState<CompanyInvoiceRow | null>(null);
  // The invoice row whose "Send på mail" ConfirmDialog is open (#429).
  const [sendingEmail, setSendingEmail] = useState<CompanyInvoiceRow | null>(null);
  // The invoice row whose "Send rykker" ConfirmDialog is open (#434).
  const [sendingReminder, setSendingReminder] = useState<CompanyInvoiceRow | null>(null);
  // Whether the rykker dialog's "Bogfør rykkergebyr nu"-checkbox is on (#434).
  // Default ON because the typical SMB owner WANTS the fee booked — it's the
  // whole point of having a registered reminder for the legal trail.
  const [reminderBookFee, setReminderBookFee] = useState(true);
  const currentScope = `${slug}:${year ?? state.data?.selectedYear ?? "default"}`;
  const inCurrentScope = interactionScope === currentScope;
  useEffect(() => {
    setInteractionScope(currentScope);
    setRegisteringPublic(false);setPublicNotice(null);
    setSettlingImported(null); setReminderClaimsId(null); setSettling(null); setCrediting(null); setSendingPublic(null); setCheckingPublicStatus(null); setSendingEmail(null); setSendingReminder(null); setReminderBookFee(true);
  }, [currentScope]);

  const q = params.get("q") ?? "";
  const status = params.get("status") ?? "all";
  const from = params.get("from") ?? "";
  const to = params.get("to") ?? "";
  const sort = params.get("sort");
  const dir = params.get("dir") === "desc" ? -1 : 1;
  const filtered = useMemo(() => {
    const rows = (state.data?.invoices ?? []).filter((row) => {
      if (detail) return row.documentId === Number(documentId);
      const needle = q.trim().toLocaleLowerCase("da");
      return (!needle || [row.invoiceNo, row.customerName ?? ""].some((value) => value.toLocaleLowerCase("da").includes(needle))) &&
        (status === "all" || !Object.hasOwn(STATUS_META, status) || row.status === status) &&
        (!from || Boolean(row.invoiceDate && row.invoiceDate >= from)) &&
        (!to || Boolean(row.invoiceDate && row.invoiceDate <= to));
    });
    if (sort === "date" || sort === "amount" || sort === "due") rows.sort((a, b) => dir * (sort === "amount" ? (a.grossAmountDkk ?? a.grossAmount) - (b.grossAmountDkk ?? b.grossAmount) : (sort === "date" ? a.invoiceDate ?? "" : a.effectiveDueDate ?? "").localeCompare(sort === "date" ? b.invoiceDate ?? "" : b.effectiveDueDate ?? "")));
    return rows;
  }, [state.data?.invoices, detail, documentId, q, status, from, to, sort, dir]);
  const pagination = listPagination(params, filtered.length);
  const visible = detail ? filtered : filtered.slice(pagination.offset, pagination.offset + pagination.pageSize);
  function setListParam(key: string, value: string) {
    const next = new URLSearchParams(params);
    if (value === "" || value === "all") next.delete(key); else next.set(key, value);
    if (key !== "page") next.delete("page");
    setParams(next, { replace: true });
  }
  function clearFilters() {
    const next = new URLSearchParams(params);
    for (const key of ["q", "status", "from", "to", "page"]) next.delete(key);
    setParams(next, { replace: true });
  }
  function toggleSort(key: string) {
    const next = new URLSearchParams(params);
    next.set("sort", key); next.set("dir", sort === key && dir === 1 ? "desc" : "asc"); next.delete("page");
    setParams(next, { replace: true });
  }

  if (state.loading && !state.data)
    return <Loading label="Henter fakturaer…" />;
  if (state.error && !state.data)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const inv = state.data!;
  const currency = inv.company.currency || "DKK";
  const invoiceCurrencies = new Set(inv.invoices.map((row) => (row.currency || currency).trim().toUpperCase()));
  const multipleCurrencies = !inv.totalsCurrency && invoiceCurrencies.size > 1;
  const totalsCurrency = inv.totalsCurrency || (invoiceCurrencies.size === 1 ? [...invoiceCurrencies][0]! : currency);

  return (
    <section {...stylex.props(cockpitStyles.statement)}>
      {state.error && <Banner kind="warning">Status kunne ikke opdateres: {state.error} De tidligere hentede fakturaer vises fortsat.</Banner>}
      <PageHeader title={detail ? `Faktura ${filtered[0]?.invoiceNo ?? documentId ?? ""}` : "Fakturaer"}
        description={`${inv.company.name} · ${currency} · Regnskabsår ${inv.selectedYear}`}
        actions={detail ? <ButtonLink  to={listReturnTo(slug, "fakturaer", params.get("returnTo"), inv.selectedYear)}>Tilbage til fakturaer</ButtonLink> : !inv.archived && can("company.draft.write") ? <ButtonLink  to={workflowTo(slug, "fakturaer", "ny", params, inv.selectedYear)} xstyle={[cockpitStyles.statementBtnComposition2]}>Udsted faktura</ButtonLink> : undefined} />

      {!detail&&<ImportedReceivablesPanel slug={slug} />}
      {publicNotice&&<Banner kind="warning">{publicNotice}</Banner>}
      {inv.nativeSales&&!inv.archived&&can("company.external-send")&&<div {...stylex.props(cockpitStyles.card)}>
        <h2>E-faktura</h2>
        {publicConfig.loading&&<p>Henter opsætning…</p>}
        {publicConfig.error&&<p>Opsætningen kunne ikke hentes. <Button variant="secondary" onClick={publicConfig.reload}>Hent opsætning igen</Button></p>}
        {!publicConfig.loading&&!publicConfig.error&&publicConfig.data&&(!publicConfig.data.configured?
          <p {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
					cockpitStyles.statementAsof,
				)}>E-faktura er ikke konfigureret. Kontakt den ansvarlige for opsætningen.</p>:
          publicConfig.data.registered&&publicConfig.data.outbound?<p {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
								)}>E-faktura er klar{publicConfig.data.environment==="test"?" i testmiljø":""}. Hver faktura sendes kun efter din bekræftelse.</p>:
          !inv.company.cvr?<p {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}>Udfyld virksomhedens navn og CVR i virksomhedsopsætningen før tilmelding.</p>:
          <><p {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}>Tilmeld {inv.company.name}{inv.company.cvr?` · CVR ${inv.company.cvr}`:""} til afsendelse af e-faktura{publicConfig.data.environment==="test"?" i testmiljø":""}.</p><Button variant="secondary" onClick={()=>{setInteractionScope(currentScope);setRegisteringPublic(true);}}>Aktivér e-faktura</Button></>)}
      </div>}
      {registeringPublic&&inCurrentScope&&<ConfirmDialog title="Aktivér e-faktura" operationKey="public-registration" confirmLabel="Bekræft tilmelding" confirmKind="danger"
        body={<p {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}>Tilmeld <strong>{inv.company.name}</strong>{inv.company.cvr?` (CVR ${inv.company.cvr})`:""} til afsendelse via DigiSense{publicConfig.data?.environment==="test"?" i testmiljø":""}. Ingen faktura sendes ved tilmeldingen.</p>}
        onConfirm={async()=>{const company=(await api.invoices(slug,year)).company;if(company.name!==inv.company.name||company.cvr!==inv.company.cvr)throw new ApiError("conflict","Virksomhedens oplysninger er ændret. Hent oversigten igen før tilmelding.",409);const fresh=await api.publicEInvoiceConfig(slug);if(!fresh.configured)throw new ApiError("bad_request","E-faktura er ikke konfigureret.",400);if(!fresh.registered||!fresh.outbound)await api.registerPublicEInvoice(slug);publicConfig.reload();}}
        onRefresh={refreshPublic} onClose={()=>setRegisteringPublic(false)}/>}

      {inv.nativeSales && <NativeInvoiceEmail key={currentScope} slug={slug} invoices={inv.invoices} archived={inv.archived} onChanged={state.reload} />}

      <CompanyNav
        slug={slug}
        years={inv.fiscalYears}
        selectedYear={inv.selectedYear}
        onYearChange={setYear}
      />

      {reminderClaimsId!==null && inCurrentScope && <NativeReminderClaims key={`${currentScope}:${reminderClaimsId}`} slug={slug} id={reminderClaimsId} onChanged={state.reload} onClose={()=>setReminderClaimsId(null)}/> }
<section
					aria-label="Importerede tilgodehavender"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.statementCard,
					)}
					data-ui="statement-card"
				>
					<div
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<h3
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h3,
								)}
							>
								Importerede tilgodehavender
							</h3>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
								)}
							>
								Kildearkiv pr. {year ?? new Date().getFullYear()}-12-31 — ikke
								Rentemester-udstedte fakturaer.
							</p>
						</div>
						<strong
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{imported.data
								? formatKroner(imported.data.totalOpen, currency)
								: "—"}
						</strong>
					</div>
					{imported.error ? (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Kunne ikke hente importarkivet.
						</p>
					) : imported.data?.rows.length ? (
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.tableScroll,
							)}
							data-ui="table-scroll"
						>
							<table
								aria-label="Kilde-fakturaer"
								{...stylex.props(
									cockpitStyles.statementTableScrollTableComposition,
								)}
							>
								<thead
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableThead,
									)}
								>
									<tr
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTr,
										)}
									>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition3,
											)}
										>
											Kilde-faktura
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition3,
											)}
										>
											Kunde
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition3,
											)}
										>
											Dato
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition4,
											)}
										>
											Åben saldo
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition3,
											)}
										>
											Handling
										</th>
									</tr>
								</thead>
								<tbody
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTbody,
									)}
								>
									{imported.data.rows.map((row) => (
										<tr
											key={`${row.scheduleHash}:${row.externalInvoiceId}`}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.responsiveTableTr,
											)}
										>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition4,
												)}
											>
												{row.externalInvoiceId}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition4,
												)}
											>
												{row.customerName ?? "—"}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition4,
												)}
											>
												{formatDateDa(row.invoiceDate)}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTdNumComposition5,
												)}
											>
												{formatKroner(row.openBalance, currency)}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition4,
												)}
											>
												{row.openBalance > 0 &&
												!inv.archived &&
												can("company.ledger.post") ? (
													<Button
														type="button"
														onClick={() => setSettlingImported(row)}
														variant={"secondary"}
														xstyle={[cockpitStyles.statementBtnComposition]}
													>
														Afstem bankpost
													</Button>
												) : (
													"—"
												)}
											</td>
										</tr>
									))}
								</tbody>
							</table>
						</div>
					) : (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Ingen importerede tilgodehavender i arkivet.
						</p>
					)}
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						{imported.data?.boundary ??
							"Importarkivet holdes adskilt fra nye fakturaer for at undgå dobbelttælling."}
					</p>
				</section>
      {settling && inCurrentScope && (
        <ConfirmDialog operationKey={String(settling.documentId)}
          title="Afstem faktura mod bankbetaling"
          body={
            <p {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
              Afstem faktura <strong {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>{settling.invoiceNo}</strong> mod en
              indgående bankbetaling med samme reference. Afstemningen lægger en
              postering og kan ikke fortrydes.
              {settling.currency!=="DKK"&&<> Bankposten skal være i {settling.currency} med dokumenteret DKK-værdi. En kursforskel bogføres på bekræftede konti for valutagevinst eller valutatab.</>}
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
          onRefresh={state.reload}
          onClose={() => setSettling(null)}
        />
      )}

      {refunding && inCurrentScope && <ConfirmDialog operationKey={`refund:${refunding.documentId}`}
        title="Afstem tilbagebetaling" confirmLabel="Afstem tilbagebetaling" confirmKind="danger"
        body={<p {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Registrér den allerede gennemførte tilbagebetaling til kunden på faktura <strong {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{refunding.invoiceNo}</strong>. Kunden har {formatKroner(-refunding.openBalance, refunding.currency)} til gode. Den valgte bankpost bogføres mellem bank og fakturaens debitorkonto.{refunding.currency!=="DKK"&&<> Bankposten skal være i {refunding.currency} med dokumenteret DKK-værdi. Kursforskelle bogføres særskilt.</>}</p>}
        noteLabel="Bankreference" notePlaceholder="Referencen på bankens udbetaling"
        onConfirm={async reference => { if (!reference.trim()) throw new Error("Angiv bankreferencen på tilbagebetalingen."); await api.refundInvoice(slug, refunding.documentId, reference.trim()); state.reload(); }}
        onRefresh={state.reload} onClose={() => setRefunding(null)} />}

			{settlingImported &&
				inCurrentScope &&
				!inv.archived &&
				can("company.ledger.post") && (
					<ConfirmDialog
						operationKey={`${settlingImported.scheduleHash}:${settlingImported.externalInvoiceId}`}
						title="Afstem importeret tilgodehavende mod bankpost"
						body={
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Afstem kildefaktura{" "}
								<strong
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{settlingImported.externalInvoiceId}
								</strong>{" "}
								mod én indgående DKK-bankpost. Det opretter en append-only
								postering og kan ikke fortrydes.
							</p>
						}
						confirmLabel="Afstem bankpost"
						confirmKind="danger"
						noteLabel="Bankpost-id"
						notePlaceholder="Det numeriske ID fra bankoversigten"
						onConfirm={async (value) => {
							const bankTransactionId = Number(value.trim());
							if (
								!Number.isInteger(bankTransactionId) ||
								bankTransactionId <= 0
							)
								throw {
									code: "bad_request",
									message: "Angiv et gyldigt numerisk bankpost-id.",
								};
							const input = {
								scheduleHash: settlingImported.scheduleHash,
								externalInvoiceId: settlingImported.externalInvoiceId,
								bankTransactionId,
							};
							const plan = await api.planImportedReceivableSettlement(
								slug,
								input,
							);
							if (!plan.ok || !plan.plan?.planHash)
								throw {
									code: "bad_request",
									message:
										"Afregningen blev afvist. Kontrollér bankpost, valuta, beløb og åben saldo.",
								};
							const result = await api.applyImportedReceivableSettlement(slug, {
								...input,
								planHash: plan.plan.planHash,
								idempotencyKey: crypto.randomUUID(),
							});
							if (!result.ok)
								throw {
									code: "bad_request",
									message:
										result.errors?.join("; ") || "Afregningen blev afvist.",
								};
							await imported.reload();
						}}
						onRefresh={() => {
							imported.reload();
							state.reload();
						}}
						onClose={() => setSettlingImported(null)}
					/>
				)}

      {/* #412: the Krediter ConfirmDialog. A credit note appends a reversing
          journal entry (and a new credit-note document), so the action is
          write-irreversible. A begrundelse is required for the audit trail —
          a blank value blocks the call before it reaches the server. */}
      {crediting && inCurrentScope && (
        <ConfirmDialog operationKey={String(crediting.documentId)}
          title="Udsted kreditnota"
          body={
            <><p {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
              Kreditér faktura <strong {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>{crediting.invoiceNo}</strong>. En
              kreditnota bogføres som modgående postering med eget nummer fra
              kreditnota-serien. Handlingen kan ikke fortrydes og kræver en
              begrundelse til revisionssporet.
            </p>
            {inv.nativeSales && <label>Beløb inkl. moms (valgfrit)<Input inputMode="decimal" value={creditGross} onChange={e => setCreditGross(e.target.value)} placeholder="Tomt felt krediterer hele restbeløbet" /></label>}
            </>
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
            const amount = creditGross.trim() ? parseDanishAmount(creditGross) : undefined;
            if (amount === null || (amount !== undefined && amount <= 0)) throw new Error("Angiv et positivt kreditbeløb.");
            await api.creditInvoice(slug, {
              ...(amount !== undefined ? { grossAmount: amount } : {}),
              invoiceDocumentId: crediting.documentId,
              // Use the LOCAL date — `toISOString()` is UTC and would mis-date
              // the BOOKED credit note in Danish evening hours (UTC+1/+2).
              issueDate: todayIso(),
              reason: reason.trim(),
            });
            state.reload();
          }}
          onRefresh={state.reload}
          onClose={() => setCrediting(null)}
        />
      )}

      {/* #428: Send e-faktura ConfirmDialog. The action is only ever
          offered for rows with an EAN-number on a public-recipient buyer;
          the dialog shows that EAN + the kanal so the owner can sanity-check
          who and where the invoice will be transmitted to. Write-irreversible
          (it records a peppol_submissions row + an audit_log entry), so the
          server requires `confirm: true` — the dialog's primary button maps
          to that flag.
          The server resolves the selected company's DigiSense identity and
          transmits now; no transport identity or credentials come from the UI. */}
      {sendingPublic && inCurrentScope && (
        <ConfirmDialog operationKey={String(sendingPublic.documentId)}
          title="Send e-faktura"
          body={
            <div {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
              <p {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
                Send faktura <strong {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{sendingPublic.invoiceNo}</strong> til{" "}
                <strong {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{sendingPublic.customerName ?? "modtageren"}</strong> som
                e-faktura via NemHandel/PEPPOL.
              </p>
              <dl {...stylex.props(...[cockpitStyles.muted])}>
                <div {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
                  <dt {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>EAN-nummer</dt>
                  <dd {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>{sendingPublic.buyerEanNumber ?? "—"}</dd>
                </div>
                <div {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
                  <dt {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Kanal</dt>
                  <dd {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>DigiSense via NemHandel (PEPPOL)</dd>
                </div>
                <div {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
                  <dt {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Handling</dt>
                  <dd {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Sendes nu</dd>
                </div>
              </dl>
              <p {...stylex.props(cockpitStyles.muted)}>
                Fakturaen sendes via DigiSense. Leveringsstatus vises bagefter;
                en køsat faktura kan kun statuskontrolleres og sendes aldrig igen.
              </p>
            </div>
          }
          confirmLabel="Send e-faktura"
          confirmKind="danger"
          onConfirm={async () => {
            // A previously opened dialog never authorizes a second delivery.
            // Also require canonical readback after any local unknown outcome.
            const fresh=await api.invoices(slug,year);
            const row=fresh.invoices.find(row=>row.documentId===sendingPublic.documentId);
            if(fresh.archived||!row||row.invoiceNo!==sendingPublic.invoiceNo||row.buyerEanNumber!==sendingPublic.buyerEanNumber||!canSendPublicInvoice(row))
              throw new ApiError("conflict","Fakturaens leveringsstatus er ændret. Kontrollér oversigten; send ikke igen.",409);
            try {
              const result=await api.sendInvoiceAsEInvoice(slug,{invoiceDocumentId:row.documentId});
              if(!result||!["prepared","queued","acknowledged","failed"].includes(result.status??"")||(inv.nativeSales&&!result.transmissionId)) {
                setPublicNotice("E-fakturaens leveringsresultat er uafklaret. Kontrollér status og afklar med modtagerens tjeneste; send ikke igen.");
                throw new ApiError("internal","Leveringsresultatet er uafklaret. Send ikke igen.",200);
              }
              if(result.status==="failed")setPublicNotice("E-fakturaen er afvist efter modtagelse. Send ikke igen.");
            } finally {state.reload();}
          }}
          onRefresh={refreshPublic}
          onClose={() => setSendingPublic(null)}
        />
      )}

      {checkingPublicStatus && inCurrentScope && (
        <ConfirmDialog operationKey={String(checkingPublicStatus.documentId)}
          title="Opdatér leveringsstatus"
          body={<p {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>Kontrollér leveringsstatus for <strong {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>{checkingPublicStatus.invoiceNo}</strong>. Handlingen observerer kun den eksisterende DigiSense-afsendelse og sender ikke fakturaen igen.</p>}
          confirmLabel="Kontrollér status"
          confirmKind="danger"
          onConfirm={async () => {
            const fresh=await api.invoices(slug,year);
            const row=fresh.invoices.find(row=>row.documentId===checkingPublicStatus.documentId);
            if(fresh.archived||!row||!canObservePublicInvoice(row))throw new ApiError("conflict","Leveringsstatus er ændret. Kontrollér oversigten.",409);
            await api.refreshEInvoiceStatus(slug, { invoiceDocumentId: row.documentId });
            state.reload();
          }}
          onRefresh={refreshPublic}
          onClose={() => setCheckingPublicStatus(null)}
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
      {sendingEmail && inCurrentScope && (
        <ConfirmDialog operationKey={String(sendingEmail.documentId)}
          title="Send faktura på mail"
          body={
            <div {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
              <p {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
                Send faktura <strong {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{sendingEmail.invoiceNo}</strong> til{" "}
                <strong {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{sendingEmail.customerName ?? "modtageren"}</strong>{" "}
                med PDF'en vedhæftet.
              </p>
              <dl {...stylex.props(...[cockpitStyles.muted])}>
                <div {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
                  <dt {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Emne</dt>
                  <dd {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Faktura {sendingEmail.invoiceNo}</dd>
                </div>
                <div {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
                  <dt {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Vedhæftning</dt>
                  <dd {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>{sendingEmail.invoiceNo}.pdf</dd>
                </div>
              </dl>
              <p {...stylex.props(cockpitStyles.muted)}>
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
          onRefresh={state.reload}
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
      {sendingReminder && inCurrentScope && (() => {
        const nextSeq = (sendingReminder.lastReminderSequence ?? 0) + 1;
        const ord = nextSeq === 1 ? "1." : nextSeq === 2 ? "2." : "3.";
        return (
          <ConfirmDialog operationKey={String(sendingReminder.documentId)}
            title="Send rykker til kunden"
            body={
              <div {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
                <p {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>
                  Send <strong {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>{ord} rykker</strong> for faktura{" "}
                  <strong {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>{sendingReminder.invoiceNo}</strong> til{" "}
                  <strong {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>
                    {sendingReminder.customerName ?? "modtageren"}
                  </strong>
                  .
                </p>
                <dl {...stylex.props(...[cockpitStyles.muted])}>
                  <div {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>
                    <dt {...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}>Dage forfalden</dt>
                    <dd {...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}>{sendingReminder.overdueDays} dage</dd>
                  </div>
                  <div {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>
                    <dt {...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}>Modtager</dt>
                    <dd {...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}>{sendingReminder.customerEmail ?? "—"}</dd>
                  </div>
                  <div {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>
                    <dt {...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}>Rykkernummer</dt>
                    <dd {...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}>{ord} rykker (af maks. 3)</dd>
                  </div>
                  <div {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>
                    <dt {...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}>Rykkergebyr</dt>
                    <dd {...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}>
                      {formatKroner(REMINDER_FEE_KRONER, "DKK")} (rentel. § 9b)
                    </dd>
                  </div>
                </dl>
                <label {...stylex.props(cockpitStyles.modalCheckbox)}>
                  <Input
                    type="checkbox"
                    checked={reminderBookFee}
                    onChange={(e) => setReminderBookFee(e.target.checked)} xstyle={[cockpitStyles.modalCheckboxInputComposition3]}
                  />{" "}
                  Bogfør rykkergebyr ({formatKroner(REMINDER_FEE_KRONER, "DKK")}
                  ) i ledgeren nu
                </label>
                <p {...stylex.props(cockpitStyles.muted)}>
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
            onRefresh={state.reload}
            onClose={() => {
              setSendingReminder(null);
              setReminderBookFee(true);
            }}
          />
        );
      })()}

      {inv.archived ? (
        <ArchivedNotice year={inv.selectedYear} />
      ) : detail && filtered.length === 0 ? (<div {...stylex.props(cockpitStyles.card)} role="status"><h2 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h2,
						)}>Fakturaen findes ikke i {inv.selectedYear}</h2><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Vælg det relevante regnskabsår eller gå tilbage til fakturaoversigten.</p></div>) : inv.invoices.length === 0 ? (
        <div {...stylex.props(cockpitStyles.card, cockpitStyles.archivedNotice)}>
          <h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.archivedNoticeH3,
						)}>Ingen fakturaer endnu</h3>
          <p {...stylex.props(cockpitStyles.muted)}>
            Der er ikke udstedt salgsfakturaer i regnskabsåret{" "}
            {inv.selectedYear}. Udstedte fakturaer vises her, så snart de er
            bogført. Brug <em {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>Udsted faktura</em> for at lave en ny.
          </p>
        </div>
      ) : (
        <>
          {!detail && <div {...stylex.props(cockpitStyles.statusGrid, cockpitStyles.invoicesSummary)}>
            <div {...stylex.props(cockpitStyles.card, cockpitStyles.statusCard)}>
              <h3 {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.h3,
										cockpitStyles.statusCardH3,
									)}>Faktureret i alt</h3>
              <div {...stylex.props(cockpitStyles.statusFigure)}>
                {multipleCurrencies ? <span {...stylex.props(cockpitStyles.muted)}>Fakturaer i flere valutaer — se beløbene på den enkelte faktura.</span> : <Amount value={inv.totalGross} currency={totalsCurrency} />}
              </div>
              <p {...stylex.props(cockpitStyles.muted, cockpitStyles.statusNote)}>
                {inv.invoices.length}{" "}
                {inv.invoices.length === 1 ? "faktura" : "fakturaer"} i{" "}
                {inv.selectedYear}
              </p>
            </div>
            <div {...stylex.props(cockpitStyles.card, cockpitStyles.statusCard)}>
              <h3 {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.h3,
										cockpitStyles.statusCardH3,
									)}>Udestående</h3>
              <div
                {...stylex.props(...[cockpitStyles.statusFigure, ...(inv.totalOpen > 0 ? [cockpitStyles.statusFigureStatusAlert] : [])])}
              >
                {multipleCurrencies ? <span {...stylex.props(cockpitStyles.muted)}>Udestående vises i den enkelte fakturas valuta.</span> : <Amount value={inv.totalOpen} currency={totalsCurrency} />}
              </div>
              <p {...stylex.props(cockpitStyles.muted, cockpitStyles.statusNote)}>
                {inv.overdueCount > 0
                  ? `${inv.overdueCount} forfalden${
                      inv.overdueCount === 1 ? "" : "e"
                    }`
                  : "Ingen forfaldne fakturaer"}
              </p>
            </div>
          </div>}

          {!detail && <FilterBar activeCount={[q, from, to, status === "all" ? "" : status].filter(Boolean).length} onReset={clearFilters}>
            <label {...stylex.props(...[cockpitStyles.formField, cockpitStyles.fullWidth])}>Søg<Input type="search" value={q} placeholder="Kunde eller fakturanummer…" onChange={(event) => setListParam("q", event.target.value)} xstyle={[cockpitStyles.journalFilterFieldInputComposition2]} /></label>
            <label {...stylex.props(...[cockpitStyles.formField])}>Status<Select value={Object.hasOwn(STATUS_META, status) ? status : "all"} onChange={(event) => setListParam("status", event.target.value)} xstyle={[cockpitStyles.journalFilterFieldSelectComposition2]}><option value="all" {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Alle</option>{Object.entries(STATUS_META).map(([value, meta]) => <option key={value} value={value} {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>{meta.label}</option>)}</Select></label>
            <label {...stylex.props(...[cockpitStyles.formField])}>Fra<Input type="date" value={from} onChange={(event) => setListParam("from", event.target.value)} xstyle={[cockpitStyles.journalFilterFieldInputComposition2]} /></label>
            <label {...stylex.props(...[cockpitStyles.formField])}>Til<Input type="date" value={to} onChange={(event) => setListParam("to", event.target.value)} xstyle={[cockpitStyles.journalFilterFieldInputComposition2]} /></label>
            <label {...stylex.props(...[cockpitStyles.formField])}>Sortering<Select value={sort === "date" || sort === "amount" || sort === "due" ? sort : "default"} onChange={(event) => { const next = new URLSearchParams(params); if (event.target.value === "default") { next.delete("sort"); next.delete("dir"); } else { next.set("sort", event.target.value); next.set("dir", params.get("dir") === "desc" ? "desc" : "asc"); } next.delete("page"); setParams(next, { replace: true }); }}><option value="default" {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Nyeste faktura</option><option value="date" {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Fakturadato</option><option value="due" {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Forfaldsdato</option><option value="amount" {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Beløb</option></Select></label>
            {(sort === "date" || sort === "amount" || sort === "due") && <label {...stylex.props(...[cockpitStyles.formField])}>Rækkefølge<Select value={dir === -1 ? "desc" : "asc"} onChange={(event) => setListParam("dir", event.target.value)} xstyle={[
											cockpitStyles.journalFilterFieldSelectComposition2,
										]}><option value="asc" {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>Stigende</option><option value="desc" {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>Faldende</option></Select></label>}
          </FilterBar>}
          {!detail && <p {...stylex.props(cockpitStyles.statementAsof, cockpitStyles.muted)}>{filtered.length} af {inv.invoices.length} fakturaer matcher</p>}

          <div {...stylex.props(cockpitStyles.card, cockpitStyles.statementCard, cockpitStyles.tableScroll)}>
            <table {...stylex.props(...[cockpitStyles.tableData, cockpitStyles.tableStatementTable, cockpitStyles.dailyListTableComposition])} role="table">
              <thead role="rowgroup" {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.dailyTableThead,
								)}>
                <tr role="row" {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.dailyTableTr,
									)}>
                  <th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}>Fakturanr.</th>
                  <th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}>Kunde</th>
                  <th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}><Button  onClick={() => toggleSort("date")} aria-label="Sortér efter dato" xstyle={[cockpitStyles.statementButtonComposition]}>Dato{sort === "date" ? dir === 1 ? " ▲" : " ▼" : ""}</Button></th>
                  <th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}><Button  onClick={() => toggleSort("due")} aria-label="Sortér efter forfald" xstyle={[cockpitStyles.statementButtonComposition]}>Forfald{sort === "due" ? dir === 1 ? " ▲" : " ▼" : ""}</Button></th>
                  <th {...stylex.props(cockpitStyles.num)}><Button  onClick={() => toggleSort("amount")} aria-label="Sortér efter beløb" xstyle={[cockpitStyles.statementButtonComposition]}>Beløb inkl. moms{sort === "amount" ? dir === 1 ? " ▲" : " ▼" : ""}</Button></th>
                  <th {...stylex.props(cockpitStyles.num)}>Udestående</th>
                  <th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}>Status</th>
                  <th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}>Handlinger</th>
                </tr>
              </thead>
              <tbody role="rowgroup" {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.dailyTableTbody,
								)}>
                {visible.length === 0 && <tr role="row" {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.dailyTableTr,
										)}><td colSpan={8} {...stylex.props(cockpitStyles.emptyInline)}>Ingen fakturaer matcher filtrene.</td></tr>}
                {visible.map((row) => {
                  const knownStatus = Object.hasOwn(STATUS_META, row.status);
                  const meta = STATUS_META[row.status] ?? { label: "Ukendt status", tone: "neutral" };
                  // Settlement only makes sense while a balance is open.
                  const canSettle = (!inv.nativeSales || inv.nativeSales.settlementAvailable === true) && row.posted !== false && knownStatus && row.openBalance > 0;
                  // #412: Krediter is offered for any posted invoice that has
                  // not already been written off / refunded / fully credited.
                  // A partial credit reduces the open balance but leaves the
                  // source invoice in its open/paid/overdue state, so those
                  // remain creditable until the core refuses on "already fully
                  // credited" (mapped to a 409 by the mutation pipeline).
                  const canCredit = (!inv.nativeSales || inv.nativeSales.creditAvailable === true) && row.posted !== false && (row.creditableBalance === undefined || row.creditableBalance > 0) &&
                    knownStatus && row.status !== "credited" &&
                    row.status !== "refunded" &&
                    row.status !== "written_off";
                  // #428: A fresh send is offered only when the buyer
                  // is a public recipient with a valid EAN-number (the
                  // server-side requirement for a NemHandel/PEPPOL send).
                  // A pre-acceptance transport failure is retryable. Once the
                  // access point has returned a queued document id, only the
                  // status action is allowed — never a second delivery.
                  const nativeReady=!inv.nativeSales||Boolean(publicConfig.data?.configured&&publicConfig.data.registered&&publicConfig.data.outbound&&!publicConfig.loading&&!publicConfig.error);
                  const canSendPublic=nativeReady&&canSendPublicInvoice(row);
                  const canCheckPublicStatus=nativeReady&&canObservePublicInvoice(row);
                  // #429: "Send på mail" appears only when the customer
                  // has an e-mail on the kontaktkort — without it the
                  // dialog has no recipient to prefill, and the issue
                  // body asks the cockpit to hide the action instead of
                  // surfacing an empty form.
                  const canSendEmail = !inv.nativeSales && Boolean(row.customerEmail);
                  // #434: "Send rykker" appears only when the row is
                  // overdue AND a customer e-mail is on file AND the
                  // statutory cap of 3 reminders has not been reached
                  // (rentel. § 9b). Hidden for archived years (no live
                  // ledger to register the reminder into).
                  const reminderSeq = row.lastReminderSequence ?? 0;
                  const canSendReminder = !inv.nativeSales &&
                    row.status === "overdue" &&
                    Boolean(row.customerEmail) &&
                    reminderSeq < 3;
                  return (
                    <tr role="row" key={row.documentId} {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.dailyTableTr,
											)}>
                      <td role="cell" data-label="Faktura" {...stylex.props(...[cockpitStyles.num])}><Link to={workflowTo(slug, "fakturaer", String(row.documentId), params, inv.selectedYear)}>{row.invoiceNo}</Link></td>
                      <td role="cell" data-label="Kunde" {...stylex.props(
													cockpitStyles.dailyTableTdComposition3,
												)}>{row.customerName ?? "—"}</td>
                      <td role="cell" data-label="Dato" {...stylex.props(cockpitStyles.entryDate)}>{row.invoiceDate ?? "—"}</td>
                      <td role="cell" data-label="Forfald" {...stylex.props(cockpitStyles.entryDate)}>
                        {row.effectiveDueDate ?? "—"}
                      </td>
                      <td role="cell" data-label="Beløb inkl. moms" {...stylex.props(cockpitStyles.num)}>
                        <Amount value={row.grossAmount} currency={row.currency || currency} />
                        {row.currency!=="DKK"&&typeof row.grossAmountDkk==="number"&&<div {...stylex.props(cockpitStyles.muted)}>{formatKroner(row.grossAmountDkk,"DKK")} · kurs {row.fxRateToDkk}</div>}
                      </td>
                      <td role="cell" data-label="Udestående" {...stylex.props(cockpitStyles.num)}>
                        <Amount value={row.openBalance} currency={row.currency || currency} />
                        {row.currency!=="DKK"&&typeof row.openBalanceDkk==="number"&&<div {...stylex.props(cockpitStyles.muted)}>Bogført: {formatKroner(row.openBalanceDkk,"DKK")}</div>}
                      </td>
                      <td role="cell" data-label="Status" {...stylex.props(
													cockpitStyles.dailyTableTdComposition3,
												)}>
                        <span {...stylex.props(cockpitStyles.flag, meta.tone === "ok" ? cockpitStyles.flagOk : meta.tone === "critical" ? cockpitStyles.flagCritical : meta.tone === "warning" ? cockpitStyles.flagWarning : cockpitStyles.flagNeutral)}>
                          {row.posted === false ? "Udstedt – afventer bogføring" : meta.label}
                          {/* Singular/plural — "· 1 dage" is wrong Danish. */}
                          {row.status === "overdue" && row.overdueDays > 0
                            ? ` · ${row.overdueDays} ${
                                row.overdueDays === 1 ? "dag" : "dage"
                              }`
                            : ""}
                        </span>
                        {!knownStatus && <details {...stylex.props(
															cockpitStyles.element,
															cockpitStyles.focusVisible,
														)}><summary {...stylex.props(
																cockpitStyles.summaryComposition,
															)}>Teknisk status</summary><code {...stylex.props(
																cockpitStyles.element,
																cockpitStyles.focusVisible,
																cockpitStyles.code,
															)}>{row.status}</code></details>}
                        {/* #429 — surface a "Sendt {dato}" flag once the
                            invoice has been emailed from the cockpit so the
                            owner can see at a glance whether the customer
                            already got it. The date is the ISO timestamp
                            sliced to YYYY-MM-DD — the audit row carries the
                            full timestamp, the row only needs the day. */}
                        {row.lastEmailedAt && (
                          <span
                            {...stylex.props(cockpitStyles.flag)}
                            title={`Sendt på mail ${row.lastEmailedAt}`}
                          >
                            Sendt {formatDateDa(row.lastEmailedAt.slice(0, 10))}
                          </span>
                        )}
                        {/* #434 — surface the reminder sequence + date once
                            a rykker has been registered, so the owner can see
                            at a glance hvor i rykkerforløbet han er (1., 2.,
                            3. rykker) without re-reading the audit log.
                            Audit EJER-11: the badge says "registreret", not
                            "sendt" — the server registers the reminder in
                            ledger/audit log, but an actual e-mail delivery is
                            not guaranteed (SMTP may run in test mode). */}
                        {row.lastReminderAt && row.lastReminderSequence > 0 && (
                          <span
                            {...stylex.props(cockpitStyles.flag)}
                            title={`Rykker registreret ${row.lastReminderAt}`}
                          >
                            {row.lastReminderSequence}. rykker registreret{" "}
                            {formatDateDa(row.lastReminderAt.slice(0, 10))}
                          </span>
                        )}
                        {/* #428 — surface e-faktura status next to settlement
                            status so the owner can see at a glance whether
                            the invoice has been transmitted to NemHandel. */}
                        {row.peppolStatus && (
                          <span
                            {...stylex.props(...[cockpitStyles.flag, ...(row.peppolStatus.status === "acknowledged" ? [cockpitStyles.flagOk] : [cockpitStyles.flagNeutral])])}
                            title={
                              row.peppolStatus.acknowledgedAt
                                ? `Bekræftet ${row.peppolStatus.acknowledgedAt}`
                                : `Reference ${row.peppolStatus.submissionReference}`
                            }
                          >
                            {row.peppolStatus.legacy||row.peppolStatus.status === "legacy"
                              ? "Tidligere e-faktura — afklar manuelt"
                              : row.peppolStatus.redeliveryBlocked&&row.peppolStatus.status === "retryable"
                              ? "E-faktura kræver afklaring — send ikke igen"
                              : row.peppolStatus.status === "acknowledged"
                              ? "E-faktura leveret"
                              : (row.peppolStatus.status === "queued" || row.peppolStatus.status === "prepared")
                                ? "E-faktura køsat — afventer status"
                                : row.peppolStatus.status === "failed"
                                  ? "E-faktura afvist — send ikke igen"
                                : row.peppolStatus.status === "uncertain"
                                  ? "E-faktura-status ukendt — afklar manuelt"
                                : row.peppolStatus.status === "in_progress"
                                  ? "E-faktura afsendes"
                                  : row.peppolStatus.status === "retryable" ? "E-faktura fejlede — kan prøves igen" : "Ukendt leveringsstatus — afklar manuelt"}
                          </span>
                        )}
                      </td>
                      <td role="cell" data-label="Handlinger" {...stylex.props(
													cockpitStyles.dailyTableTdComposition3,
												)}>
                        <div {...stylex.props(cockpitStyles.rowActions)}>
                          {/* #378: the PDF link is the primary action — the
                              whole point of issuing an invoice is to send it
                              to the customer. Download the exact original or
                              open it separately without losing the table view. */}
                          {inv.nativeSales && row.posted === false && can("company.ledger.post") && <NativeInvoicePost slug={slug} id={row.documentId} number={row.invoiceNo} accounts={inv.nativeSales.revenueAccounts} onPosted={state.reload} />}
                          <a
                            {...stylex.props(...[cockpitStyles.buttonComposition, cockpitStyles.element])}
                            href={api.invoicePdfUrl(slug, row.documentId)}
                            download={`faktura-${row.documentId}.pdf`}
                          >
                            Hent PDF
                          </a>
                          <a
                            {...stylex.props(...[cockpitStyles.buttonComposition, cockpitStyles.element])}
                            href={api.invoicePdfUrl(slug, row.documentId)}
                            target="_blank"
                            rel="noopener"
                          >
                            Åbn PDF
                          </a>
                          {!inv.archived && inv.nativeSales?.refundAvailable && row.posted === true && row.openBalance < 0 && can("company.ledger.post") && <Button type="button" variant="secondary" onClick={() => { setInteractionScope(currentScope); setRefunding(row); }}>Afstem tilbagebetaling</Button>}
                          {inv.nativeSales && !inv.archived && row.posted && <Button type="button" variant="secondary" onClick={()=>{setInteractionScope(currentScope);setReminderClaimsId(row.documentId);}}>Rykkergebyrer og krav</Button>}
                          {!inv.archived && can("company.ledger.post") && canSettle && (
                            <Button
                              type="button"

                              onClick={() => { setInteractionScope(currentScope); setSettling(row); }}
                            >
                              Afstem
                            </Button>
                          )}
                          {/* #412: per-row Krediter button. The action is
                              hidden for an archived (read-only) year — every
                              write-action in this view is — and for rows
                              already credited/refunded/written off. */}
                          {!inv.archived && can("company.ledger.post") && canCredit && (
                            <Button
                              type="button"

                              onClick={() => { setInteractionScope(currentScope); setCreditGross(""); setCrediting(row); }}
                            >
                              Kreditér
                            </Button>
                          )}
                          {/* #428 — "Send e-faktura" is shown ONLY when
                              the customer has an EAN-number on file (a public
                              buyer). Hidden for archived years and once the
                              invoice has been acknowledged by the access
                              point. A queued delivery has its own status-only
                              action and can never be redelivered from here. */}
                          {!inv.archived && can("company.external-send") && canSendPublic && (
                            <Button
                              type="button"

                              onClick={() => { setInteractionScope(currentScope); setSendingPublic(row); }}
                            >
                              Send e-faktura
                            </Button>
                          )}
                          {!inv.archived && can("company.external-send") && canCheckPublicStatus && (
                            <Button type="button"  onClick={() => { setInteractionScope(currentScope); setCheckingPublicStatus(row); }}>
                              Opdatér leveringsstatus
                            </Button>
                          )}
                          {/* #429 — "Send på mail" is shown ONLY when the
                              customer has an e-mail on the kontaktkort.
                              Hidden for archived years (no live ledger).
                              The button replaces the missing CLI step
                              `invoice send` so SMB owners no longer have to
                              download the PDF and open their own mail
                              client to get the invoice out to the customer. */}
                          {!inv.archived && can("company.external-send") && canSendEmail && (
                            <Button
                              type="button"

                              onClick={() => { setInteractionScope(currentScope); setSendingEmail(row); }}
                            >
                              Send på mail
                            </Button>
                          )}
                          {/* #434 — "Send rykker" is shown ONLY for overdue
                              rows where the customer has an e-mail AND the
                              statutory 3-reminder cap (rentel. § 9b) has
                              not been reached. Hidden for archived years
                              (no live ledger). One click opens the
                              ConfirmDialog with the recipient, days
                              overdue, reminder number and a fee-booking
                              checkbox. */}
                          {!inv.archived && can("company.external-send") && canSendReminder && (
                            <Button
                              type="button"

                              onClick={() => { setInteractionScope(currentScope); setSendingReminder(row); }}
                            >
                              Send rykker
                            </Button>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {!detail && <Pagination total={filtered.length} {...pagination} onPageChange={(page) => setListParam("page", String(page))} onPageSizeChange={(size) => setListParam("pageSize", String(size))} />}
        </>
      )}
    </section>
  );
}

function ArchivedNotice({ year }: { year: string }) {
  return (
    <div {...stylex.props(cockpitStyles.card, cockpitStyles.archivedNotice)}>
      <h3 {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
					cockpitStyles.archivedNoticeH3,
				)}>Fakturaer er ikke tilgængelige for {year}</h3>
      <p {...stylex.props(cockpitStyles.muted)}>
        {year} er et arkiveret regnskabsår. Udstedte fakturaer føres kun i den
        aktive ledger og vises derfor ikke for et arkiveret år.
        Resultatopgørelse, balance, saldobalance og posteringer for {year} er
        tilgængelige.
      </p>
    </div>
  );
}

export function InvoiceDetailView() { return <InvoicesView detail />; }
