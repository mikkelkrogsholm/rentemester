import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useOptionalAuth } from "../lib/auth-context";
import { Button, Dialog, Input, Select } from "./ui";
import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../lib/api";
import { ApiError } from "../lib/api/_shared";
import type { CompanyPayables, DirectBankPayableCorrectionInput, DirectBankPayableCorrectionPlan } from "../lib/types";

type Props = { slug: string; payables: CompanyPayables; onApplied: () => void; onClose: () => void };
type Treatment = "standard" | "exempt" | "non_deductible";
type Reviewed = { input: DirectBankPayableCorrectionInput; plan: DirectBankPayableCorrectionPlan };
type Attempt = DirectBankPayableCorrectionInput & { planHash: string; reason: string; idempotencyKey: string };
const TREATMENTS: Record<Treatment, string> = { standard: "Almindeligt momsfradrag", exempt: "Momsfrit", non_deductible: "Ingen fradragsret" };
const hash = (value: unknown) => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const positive = (value: unknown) => typeof value === "number" && Number.isSafeInteger(value) && value > 0;

/** A company/session change discards local source reads, never adopts another form's review. */
export function DirectBankPayableCorrectionModal(props: Props) {
  const auth = useOptionalAuth();
  const native = auth?.backend === "rust";
  const scope = `${window.location.origin}:${auth?.session?.id ?? ""}:${auth?.currentSessionId ?? ""}:${props.slug}`;
  return <CorrectionForm key={scope} {...props} native={native} scope={scope} />;
}
function CorrectionForm({ slug, payables, onApplied, onClose: onDismiss, native, scope }: Props & { native: boolean; scope: string }) {
  const dedicated = native || payables.directBankCorrectionCandidates !== undefined;
  const candidates = payables.directBankCorrectionCandidates ?? [];
  const legacyDocument = payables.unregisteredDocuments[0];
  const [choice, setChoice] = useState(dedicated ? "" : String(legacyDocument?.id ?? ""));
  const selected = candidates.find(row => `${row.documentId}:${row.bankTransactionId}` === choice);
  const legacy = payables.unregisteredDocuments.find(row => String(row.id) === choice);
  const [bankTransactionId, setBankTransactionId] = useState("");
  const [billDate, setBillDate] = useState(dedicated ? "" : legacyDocument?.invoiceDate ?? "");
  const [dueDate, setDueDate] = useState(dedicated ? "" : legacyDocument?.invoiceDate ?? "");
  const [expenseAccountNo, setExpenseAccountNo] = useState(dedicated ? "" : payables.expenseAccounts[0]?.accountNo ?? "");
  const [vatTreatment, setVatTreatment] = useState<Treatment | "">(dedicated ? "" : "standard");
  const [reason, setReason] = useState(dedicated ? "" : "Ret direkte bankkøb til kreditorforløb");
  const [reviewed, setReviewed] = useState<Reviewed | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false), [error, setError] = useState<string | null>(null);
  const read = useRef<AbortController | null>(null), serial = useRef(0), alive = useRef(true), sending = useRef(false);
  const attempted = useRef<Attempt>();
  const outcome = useMutationOutcome(onApplied, native ? "direct-bank-correction" : "form", native ? scope : undefined);
  const dirty = dedicated ? Boolean(choice || bankTransactionId || reviewed || reason || dueDate || expenseAccountNo) : Boolean(bankTransactionId || reviewed) || choice !== String(legacyDocument?.id ?? "") || dueDate !== (legacyDocument?.invoiceDate ?? "") || expenseAccountNo !== (payables.expenseAccounts[0]?.accountNo ?? "") || reason !== "Ret direkte bankkøb til kreditorforløb";
  const guard = useDiscardGuard(dirty, onDismiss);
  const onClose = () => { if (!busy) guard.onClose(); };
  useEffect(() => { alive.current = true; return () => { alive.current = false; serial.current++; read.current?.abort(); }; }, []);
  const wasBlocked = useRef(outcome.blocked);
  useEffect(() => {
    if (native && wasBlocked.current && !outcome.blocked) { setReviewed(null); setConfirmed(false); attempted.current = undefined; }
    wasBlocked.current = outcome.blocked;
  }, [native, outcome.blocked]);
  function invalidate() { read.current?.abort(); serial.current++; setReviewed(null); setConfirmed(false); attempted.current = undefined; }
  function select(value: string) {
    invalidate(); setChoice(value);
    const candidate = candidates.find(row => `${row.documentId}:${row.bankTransactionId}` === value);
    setBankTransactionId(dedicated ? String(candidate?.bankTransactionId ?? "") : "");
    setBillDate(dedicated ? candidate?.invoiceDate ?? "" : payables.unregisteredDocuments.find(row => String(row.id) === value)?.invoiceDate ?? "");
    if (dedicated) setDueDate("");
  }
  const documentId = dedicated ? selected?.documentId ?? 0 : legacy?.id ?? 0;
  const disabled = busy || outcome.blocked;
  async function review() {
    if (disabled || !vatTreatment || (dedicated && !selected)) return;
    read.current?.abort(); const controller = new AbortController(); read.current = controller;
    const version = ++serial.current;
    const input: DirectBankPayableCorrectionInput = { documentId, bankTransactionId: Number(bankTransactionId), billDate, dueDate, expenseAccountNo, vatTreatment };
    setBusy(true); setError(null); setReviewed(null); setConfirmed(false);
    try {
      const plan = await api.planDirectBankPayableCorrection(slug, input, { signal: controller.signal });
      if (!alive.current || version !== serial.current) return;
      if (native && (!plan.summary || plan.summary.document.id !== input.documentId || plan.summary.bank.id !== input.bankTransactionId || plan.summary.originalJournal.id !== plan.originalJournalEntryId || plan.summary.originalJournal.hash !== plan.originalJournalHash || plan.summary.expenseAccount.accountNo !== input.expenseAccountNo || plan.summary.effectiveVatTreatment !== input.vatTreatment || !hash(plan.planHash) || !hash(plan.documentHash) || !hash(plan.originalJournalHash) || !positive(plan.originalJournalEntryId) || !Number.isFinite(plan.bankAmount) || Object.entries(input).some(([field, value]) => plan[field as keyof DirectBankPayableCorrectionPlan] !== value))) throw new Error("Serverens plan svarer ikke til de valgte kilder. Ingen korrektion er sendt.");
      attempted.current = undefined; setReviewed({ input, plan });
    } catch (cause) { if (alive.current && version === serial.current) setError(cause instanceof Error ? cause.message : "Planen kunne ikke oprettes."); }
    finally { if (alive.current && version === serial.current) setBusy(false); }
  }
  async function apply() {
    if (sending.current || outcome.isBlocked() || !reviewed || (native && !confirmed) || !reason.trim()) return;
    if (!alive.current) return;
    if (!attempted.current) attempted.current = { ...reviewed.input, planHash: reviewed.plan.planHash, reason, idempotencyKey: crypto.randomUUID() };
    // One frozen request/key remains unchanged when its result is uncertain.
    const body = attempted.current;
    sending.current = true; setBusy(true); setError(null);
    try {
      const result = await api.applyDirectBankPayableCorrection(slug, body);
      if (native && (result.ok !== true || ![result.id, result.payableId, result.paymentId, result.settlementJournalEntryId].every(positive) || typeof result.idempotent !== "boolean" || !Array.isArray(result.errors) || result.errors.length !== 0)) throw new ApiError("internal", "Serverens kvittering kunne ikke bekræftes. Kontrollér kreditor, bank og posteringer før næste handling.", 200);
      onApplied(); guard.dismiss();
    } catch (cause) {
      let failure = cause;
      if (native && (!(cause instanceof ApiError) || cause.status === 0 || cause.status >= 500 || [408, 409].includes(cause.status))) failure = new ApiError("internal", cause instanceof Error ? cause.message : "Resultatet kunne ikke bekræftes. Kontrollér status før næste handling.", cause instanceof ApiError ? cause.status : 200);
      if (native && failure instanceof ApiError && failure.code !== "internal" && failure.code !== "network" && failure.status >= 400 && failure.status < 500) { setReviewed(null); setConfirmed(false); attempted.current = undefined; }
      setError(failure instanceof Error ? failure.message : "Korrektionen kunne ikke gennemføres.");
      try { outcome.reject(failure); } catch { /* The retained marker and message are rendered here. */ }
    } finally { sending.current = false; if (alive.current) setBusy(false); }
  }
  const plan = reviewed?.plan;
  const sourceName = selected?.invoiceNo ?? selected?.documentNo ?? legacy?.invoiceNo ?? "";
  const accountName = plan?.summary?.expenseAccount.name ?? payables.expenseAccounts.find(row => row.accountNo === plan?.expenseAccountNo)?.name;
  return <Dialog title="Ret direkte bankkøb" onClose={onClose} busy={busy} xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}>
    {outcome.feedback}{guard.confirmation}
    <p>Korrektionen bevarer originalbilaget og den oprindelige postering. Den tilbagefører det direkte bankkøb, registrerer fakturaen på fakturadatoen og afregner på den faktiske bankdato. Der foretages ingen ny betaling eller mailafsendelse.</p>
    <Button variant="secondary" type="button" disabled={busy} onClick={onClose}>Luk</Button>
    {native && payables.directBankCorrectionCandidates === undefined && <p role="alert">Serveren har ikke leveret listen over direkte bankkøb. Almindelige uregistrerede bilag kan ikke bruges som erstatning. Genlæs kreditoroversigten.</p>}
    {dedicated && candidates.length === 0 && payables.directBankCorrectionCandidates !== undefined && <p {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}>Ingen direkte bankkøb er tilgængelige for korrektion.</p>}
    <div {...stylex.props(cockpitStyles.formGrid)}>
      <label {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.formGridLabel,
					)}>Bilag<Select aria-label="Bilag" disabled={disabled} value={choice} onChange={e => select(e.target.value)}>
        {dedicated && <option value="">Vælg et bogført bankkøb</option>}
        {dedicated ? candidates.map(row => <option key={`${row.documentId}:${row.bankTransactionId}`} value={`${row.documentId}:${row.bankTransactionId}`}>{row.invoiceNo ?? row.documentNo ?? `Bilag ${row.documentId}`} · {row.supplierName ?? "Leverandør ikke oplyst"} · bankpost {row.bankTransactionId}</option>) : payables.unregisteredDocuments.map(row => <option key={row.id} value={row.id} {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>{row.invoiceNo ?? row.documentNo ?? `Bilag ${row.id}`}</option>)}
      </Select></label>
      <label {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.formGridLabel,
					)}>Banktransaktions-id<Input aria-label="Banktransaktions-id" disabled={disabled} readOnly={dedicated} inputMode="numeric" value={bankTransactionId} onChange={e => { invalidate(); setBankTransactionId(e.target.value); }} /></label>
      <label {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.formGridLabel,
					)}>Fakturadato<Input aria-label="Fakturadato" disabled={disabled} type="date" value={billDate} readOnly aria-describedby="invoice-date-note" /></label>
      <label {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.formGridLabel,
					)}>Forfaldsdato<Input aria-label="Forfaldsdato" disabled={disabled} type="date" value={dueDate} onChange={e => { invalidate(); setDueDate(e.target.value); }} /></label>
      <label {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.formGridLabel,
					)}>Udgiftskonto<Select aria-label="Udgiftskonto" disabled={disabled} value={expenseAccountNo} onChange={e => { invalidate(); setExpenseAccountNo(e.target.value); }}>{dedicated && <option value="">Vælg udgiftskonto</option>}{payables.expenseAccounts.map(row => <option key={row.accountNo} value={row.accountNo} {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>{row.accountNo} · {row.name}</option>)}</Select></label>
      {dedicated && <label {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.formGridLabel,
					)}>Momsbehandling<Select aria-label="Momsbehandling" disabled={disabled} value={vatTreatment} onChange={e => { invalidate(); setVatTreatment(e.target.value as Treatment | ""); }}><option value="">Vælg momsbehandling</option>{Object.entries(TREATMENTS).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</Select></label>}
      <label>Begrundelse<Input aria-label="Begrundelse" disabled={disabled} value={reason} onChange={e => { setReason(e.target.value); setConfirmed(false); }} /></label>
    </div>
    <p id="invoice-date-note" {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}>Fakturadatoen kommer uændret fra bilaget {sourceName}.</p>
    {selected && <p>Leverandør: {selected.supplierName ?? "Ikke oplyst"} · Bilagsbeløb: {selected.amountIncVat} {selected.currency} · Moms i bilaget: {selected.vatAmount ?? "Ikke oplyst"} · Bankdato: {selected.bankDate}</p>}
    {error && <p role="alert" {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{error}</p>}
    {plan && <section aria-label="Gennemgå korrektionen" {...stylex.props(cockpitStyles.card)}>
      <h3>Kontrollér planen</h3>
      {plan.summary && <><p>Bilag: {plan.summary.document.invoiceNo ?? plan.summary.document.documentNo ?? plan.documentId} · {plan.summary.document.supplierName ?? "Leverandør ikke oplyst"}</p><p>Bilagsbeløb: {plan.summary.document.grossAmount} {plan.summary.document.currency} · Moms i bilaget: {plan.summary.document.vatAmount ?? "Ikke oplyst"}</p><p>Bankpost: {plan.summary.bank.text} · {plan.summary.bank.signedAmount} {plan.summary.bank.currency}</p></>}
      <p>Fakturadato {plan.billDate} · Forfaldsdato {plan.dueDate}</p>
      <p>Bankdato {plan.bankDate} · {plan.bankAmount} DKK</p>
      <p>Udgiftskonto {plan.expenseAccountNo}{accountName ? ` · ${accountName}` : ""} · {TREATMENTS[plan.vatTreatment as Treatment] ?? "Momsbehandling ikke oplyst"}</p>
      <p>Moms i originalbilaget: {plan.documentVatAmount ?? "Ikke oplyst"} DKK</p>
      <p>Oprindelig postering {plan.originalJournalEntryNo ?? plan.originalJournalEntryId} · {plan.originalJournalDate ?? "Dato ikke oplyst"}. Denne postering tilbageføres; originalen bevares.</p>
      <p><a href={`/api/companies/${encodeURIComponent(slug)}/documents/${plan.documentId}/file`} target="_blank" rel="noopener noreferrer">Åbn originalbilag</a> · <Link to={`/companies/${encodeURIComponent(slug)}/posteringer?journalEntryId=${plan.originalJournalEntryId}`} target="_blank" rel="noopener noreferrer">Åbn oprindelig postering</Link></p>
      <p>Kilder: bilag {plan.documentId} · bankpost {plan.bankTransactionId} · oprindelig postering {plan.originalJournalEntryId}</p>
      <label>Plan-hash<Input aria-label="Plan-hash" readOnly value={plan.planHash} /></label>
      <details><summary>Fuld plan og kildebeviser</summary><pre>{JSON.stringify(plan, null, 2)}</pre></details>
      {native && <label><input type="checkbox" disabled={disabled} checked={confirmed} onChange={e => setConfirmed(e.target.checked)} />Jeg har gennemgået bilaget, den oprindelige postering og korrektionen</label>}
    </section>}
    {outcome.blocked && attempted.current && <p {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}>Det uafklarede forsøg er låst til det viste bilag, bankpost, plan-hash og den oprindelige begrundelse. Genlæs kreditoroversigten og kontrollér bank og posteringer; der sendes ingen automatisk gentagelse.</p>}
    <div {...stylex.props(cockpitStyles.rowActions)}><Button variant="secondary" type="button" disabled={disabled || !documentId || !positive(Number(bankTransactionId)) || !billDate || !dueDate || !expenseAccountNo || !vatTreatment} onClick={() => void review()}>{busy ? "Arbejder…" : "Opret plan"}</Button><Button requiredPermission="company.ledger.post" variant="danger" type="button" disabled={disabled || !plan || !reason.trim() || (native && !confirmed)} onClick={() => void apply()}>Bekræft korrektion</Button></div>
  </Dialog>;
}
