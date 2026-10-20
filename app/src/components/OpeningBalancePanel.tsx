import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../lib/api";
import { openingBalanceApi } from "../lib/api/opening-balance";
import { archiveImportApi } from "../lib/api/archive-import";
import { ApiError } from "../lib/api/_shared";
import { useOptionalAuth } from "../lib/auth-context";
import { useAsync } from "../lib/useAsync";
import { useCapabilities } from "../lib/useCapabilities";
import { useMutationBlock } from "../lib/mutation-memory";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { formatKroner } from "../lib/format";
import { Button, Dialog, Input, Select } from "./ui";
import type { FiscalYearEntry } from "../lib/types/common";
import type { AccountRow } from "../lib/types/accounts";
import type { OpeningApply, OpeningInput, OpeningPlan, OpeningRecord } from "../lib/types/opening-balance";

type Line = { id: number; accountNo: string; debit: string; credit: string; text: string };
type Pending = { idempotencyKey: string; planHash: string; inputHash: string };
type Review = { plan: OpeningPlan; body: OpeningApply; inputHash: string; accounts: AccountRow[] };
const hash = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const money = (value: unknown): value is string => typeof value === "string" && /^(0|[1-9]\d*)\.\d{2}$/.test(value);
const blankLine = (id: number): Line => ({ id, accountNo: "", debit: "", credit: "", text: "" });
async function digest(input: OpeningInput) {
  return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify(input)))), n => n.toString(16).padStart(2, "0")).join("");
}
function sameInput(left: OpeningInput, right: OpeningInput) {
  // Compare source fields, independent of JSON member order. No financial arithmetic.
  const wire = (input: OpeningInput) => JSON.stringify({ cutOverDate: input.cutOverDate, lines: input.lines.map(line => [line.accountNo, line.debitAmount, line.creditAmount, line.text]), note: input.note, sourceArchiveImportIds: input.sourceArchiveImportIds });
  return wire(left) === wire(right);
}
function validRecord(record: OpeningRecord | null): record is OpeningRecord {
  return !!record && Number.isSafeInteger(record.journalEntryId) && record.journalEntryId > 0 && typeof record.journalEntryNo === "string" && hash(record.journalEntryHash) && hash(record.planHash) && record.sourceBasis?.kind === "operator_supplied" && typeof record.sourceBasis.currency === "string" && Array.isArray(record.sourceBasis.sourceArchives) && typeof record.createdAt === "string" && typeof record.actor === "string" && /^\d{4}-\d{2}-\d{2}$/.test(record.cutOverDate);
}
function sameRecord(left: OpeningRecord | null, right: OpeningRecord | null) {
  return validRecord(left) && validRecord(right) && left.journalEntryId === right.journalEntryId && left.journalEntryHash === right.journalEntryHash && left.planHash === right.planHash && left.cutOverDate === right.cutOverDate && left.journalEntryNo === right.journalEntryNo && left.createdAt === right.createdAt && left.actor === right.actor && JSON.stringify(left.sourceBasis) === JSON.stringify(right.sourceBasis);
}
function validPlan(plan: OpeningPlan, input: OpeningInput) {
  return plan && hash(plan.planHash) && typeof plan.ruleVersion === "string" && ["ready", "blocked"].includes(plan.status) && Array.isArray(plan.blockers) && money(plan.totals?.debit) && money(plan.totals?.credit) && plan.sourceBasis?.kind === "operator_supplied" && typeof plan.sourceBasis.currency === "string" && Array.isArray(plan.sourceBasis.sourceArchives) && plan.sourceBasis.sourceArchives.length === input.sourceArchiveImportIds.length && input.sourceArchiveImportIds.every(id => plan.sourceBasis.sourceArchives.some(source => source.importId === id && hash(source.sourceHash) && typeof source.parserContract === "string" && typeof source.declaredCurrency === "string")) && sameInput(plan.input, input);
}
export function OpeningBalancePanel({ slug }: { slug: string }) {
  const auth = useOptionalAuth();
  const scope = `${window.location.origin}:${auth?.session?.id ?? ""}:${slug}`;
  return <Panel key={`${scope}:${auth?.currentSessionId ?? ""}`} slug={slug} scope={scope} />;
}
function RecordSummary({ slug, record, years }: { slug: string; record: OpeningRecord; years: FiscalYearEntry[] }) {
  const year = years.find(year => year.source === "live" && year.start && year.end && year.start <= record.cutOverDate && year.end >= record.cutOverDate)?.label;
  return <section aria-label="Bogført åbningsbalance"><h4>Bogført åbningsbalance</h4><p>Skæringsdato {record.cutOverDate} · postering {record.journalEntryNo} · {record.createdAt}</p><p>Åbningsbalance og senere aktive posteringer danner det aktive regnskab. Arkivbeløb lægges ikke til.</p>{year ? <Link to={`/companies/${encodeURIComponent(slug)}/posteringer?year=${encodeURIComponent(year)}&source=live&journalEntryId=${record.journalEntryId}`}>Vis åbningspostering {record.journalEntryNo}</Link> : <p>Regnskabsåret til posteringen kunne endnu ikke bestemmes fra den aktuelle årsliste. Genlæs kilderne, eller vælg det bogførte år under Posteringer.</p>}<details><summary>Åbningsbalancens dokumentation</summary><p>Plan-hash: <code>{record.planHash}</code></p><p>Posteringshash: <code>{record.journalEntryHash}</code></p><p>Bogført af {record.actor} · {record.sourceBasis.currency}</p>{record.sourceBasis.sourceArchives.map(source => <p key={source.importId}><a href={`/api/companies/${encodeURIComponent(slug)}/archive-import/${source.importId}/original`} download>Hent åbningsbalancens arkivkilde {source.importId}</a> · {source.declaredCurrency} · {source.parserContract} · <code>{source.sourceHash}</code></p>)}</details></section>;
}
function PlanSummary({ review }: { review: Review }) {
  const { plan, body, accounts } = review;
  return <section aria-label="Gennemgå åbningsbalance"><p>Skæringsdato: <strong>{body.input.cutOverDate}</strong>. Aktuel basisvaluta: <strong>{plan.sourceBasis.currency}</strong>.</p><p>Du har selv angivet beløbene. Henvisninger til arkivet dokumenterer kilden; de kopierer ingen beløb.</p>{body.input.note && <p>Note: {body.input.note}</p>}<div className="table-scroll"><table><thead><tr><th>Konto</th><th>Linjetekst</th><th>Debet</th><th>Kredit</th></tr></thead><tbody>{body.input.lines.map((line, index) => <tr key={index}><td>{line.accountNo} · {accounts.find(account => account.accountNo === line.accountNo)?.name ?? "Navn ikke tilgængeligt"}</td><td>{line.text ?? "—"}</td><td>{formatKroner(line.debitAmount, plan.sourceBasis.currency)}</td><td>{formatKroner(line.creditAmount, plan.sourceBasis.currency)}</td></tr>)}</tbody><tfoot><tr><th colSpan={2}>Serverens totaler</th><td>{formatKroner(plan.totals.debit, plan.sourceBasis.currency)}</td><td>{formatKroner(plan.totals.credit, plan.sourceBasis.currency)}</td></tr></tfoot></table></div>{plan.blockers.map((blocker, index) => <p role="alert" key={index}>{blocker}</p>)}<details><summary>Kilde og plan-hash</summary><p>Gennemgået plan-hash: <code>{body.planHash}</code></p>{plan.planHash !== body.planHash && <p>Aktuel læseplans hash: <code>{plan.planHash}</code>. Det oprindelige forsøg ændres ikke.</p>}<p>Regelversion: <code>{plan.ruleVersion}</code></p>{plan.sourceBasis.sourceArchives.length ? plan.sourceBasis.sourceArchives.map(source => <p key={source.importId}>Arkivimport {source.importId} · {source.declaredCurrency} · {source.parserContract} · <code>{source.sourceHash}</code></p>) : <p>Ingen arkivhenvisninger valgt.</p>}</details></section>;
}
function Panel({ slug, scope }: { slug: string; scope: string }) {
  const [cutOverDate, setDate] = useState(""); const [note, setNote] = useState("");
  const [lines, setLines] = useState<Line[]>([blankLine(1), blankLine(2)]); const next = useRef(3);
  const [citations, setCitations] = useState<number[]>([]);
  const [review, setReview] = useState<Review | null>(null); const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false); const [error, setError] = useState(""); const [reconciled, setReconciled] = useState(false); const [reconciliation, setReconciliation] = useState("");
  const [done, setDone] = useState<OpeningRecord | null>(null);
  const current = useAsync(signal => openingBalanceApi.read(slug, { signal }), [slug]);
  const accounts = useAsync(signal => api.accounts(slug, { signal }), [slug]);
  const years = useAsync(signal => api.fiscalYears(slug, { signal }), [slug]);
  const archives = useAsync(signal => archiveImportApi.list(slug, { signal }), [slug]);
  const storageKey = `rentemester:opening-balance-attempt:${scope}`;
  const [pending, setPending] = useState<Pending | null>(() => {
    try { const value = JSON.parse(sessionStorage.getItem(storageKey) ?? "null"); if (value === null) return null; if (typeof value.idempotencyKey !== "string" || !hash(value.planHash) || !hash(value.inputHash)) throw Error(); return value; }
    catch { return { idempotencyKey: "", planHash: "", inputHash: "" }; }
  });
  const marker = useMutationBlock(`${scope}:opening-balance`); const caps = useCapabilities(slug);
  const alive = useRef(true), working = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const guard = useDiscardGuard(Boolean(review || pending || cutOverDate || note || lines.some(line => line.accountNo || line.debit || line.credit || line.text)), () => { setReview(null); setConfirmed(false); });
  const posted = done ?? current.data?.openingBalance ?? null;
  function change(id: number, patch: Partial<Line>) { setLines(old => old.map(line => line.id === id ? { ...line, ...patch } : line)); }
  async function makePlan() {
    if (working.current || review || marker.isBlocked() && !pending) return;
    working.current = true; setBusy(true); setError(""); setReconciled(false); setReconciliation("");
    try {
      if (!current.data || current.error || !accounts.data || accounts.error) throw Error("Læs først den aktuelle åbningsbalance og kontoplan.");
      if (!/^\d{4}-\d{2}-\d{2}$/.test(cutOverDate)) throw Error("Vælg skæringsdatoen.");
      if (lines.length < 2 || lines.some(line => !line.accountNo || !accounts.data!.accounts.some(account => account.accountNo === line.accountNo && account.active !== false))) throw Error("Vælg aktuelle konti for alle linjer.");
      const input: OpeningInput = { cutOverDate, lines: lines.map(line => {
        const debitAmount = line.debit || "0.00", creditAmount = line.credit || "0.00";
        if (!money(debitAmount) || !money(creditAmount)) throw Error("Angiv beløb med punktum og præcis to decimaler, f.eks. 100.00. Lad den anden side være tom.");
        return { accountNo: line.accountNo, debitAmount, creditAmount, text: line.text || null };
      }), note: note || null, sourceArchiveImportIds: [...citations].sort((a, b) => a - b) };
      if (new TextEncoder().encode(JSON.stringify(input)).length > 64 * 1024) throw Error("Åbningsbalancen er større end 64 KiB.");
      const inputHash = await digest(input);
      if (pending && pending.inputHash !== inputHash) throw Error("Det uafklarede forsøg kræver præcis samme dato, linjer, note og arkivhenvisninger. Der oprettes ingen ny nøgle.");
      const plan = await openingBalanceApi.plan(slug, input);
      if (!alive.current) return;
      if (!validPlan(plan, input)) throw Error("Serverens plan stemmer ikke med de angivne kilder og linjer.");
      setReview({ plan, inputHash, accounts: accounts.data.accounts, body: { input, planHash: pending?.planHash ?? plan.planHash, idempotencyKey: pending?.idempotencyKey ?? crypto.randomUUID(), confirm: true } }); setConfirmed(false);
    } catch (cause) { if (alive.current) setError(cause instanceof Error ? cause.message : "Åbningsbalancen kunne ikke gennemgås."); }
    finally { working.current = false; if (alive.current) setBusy(false); }
  }
  async function reconcile() {
    if (!review || working.current) return; working.current = true; setBusy(true); setError(""); setReconciled(false);
    try {
      const result = await openingBalanceApi.read(slug);
      if (result.openingBalance && (!validRecord(result.openingBalance) || result.openingBalance.planHash !== review.body.planHash || result.openingBalance.cutOverDate !== review.body.input.cutOverDate)) throw Error("Den aktuelle åbningsbalance tilhører en anden plan. Forsøget kan ikke gentages her.");
      if (!alive.current) return;
      setReconciled(true); setConfirmed(false); setReconciliation(result.openingBalance ? "Den bogførte åbningsbalance svarer til planen. Hent den oprindelige kvittering med præcis samme anmodning; der bogføres ikke en ny åbningsbalance." : "Ingen åbningsbalance er registreret nu. Det uafklarede forsøg kan stadig være gennemført. Et særskilt forsøg bruger præcis samme anmodning og nøgle.");
    } catch (cause) { if (alive.current) setError(cause instanceof Error ? cause.message : "Status kunne ikke kontrolleres."); }
    finally { working.current = false; if (alive.current) setBusy(false); }
  }
  async function apply() {
    if (!review || !confirmed || working.current || !caps.can("company.admin") || !caps.can("company.ledger.post") || pending && !reconciled || !pending && (review.plan.status !== "ready" || review.plan.blockers.length || review.plan.existing)) return;
    working.current = true; setBusy(true); setError("");
    let confirmedReceipt = false;
    try {
      if (new TextEncoder().encode(JSON.stringify(review.body)).length > 64 * 1024) throw Error("Den bekræftede anmodning er større end 64 KiB. Intet er sendt.");
      const attempt: Pending = { idempotencyKey: review.body.idempotencyKey, planHash: review.body.planHash, inputHash: review.inputHash };
      const serialized = JSON.stringify(attempt); sessionStorage.setItem(storageKey, serialized);
      if (sessionStorage.getItem(storageKey) !== serialized || !marker.block()) throw Error("Forsøgets private sikkerhedsmarkør kunne ikke gemmes. Intet er sendt.");
      setPending(attempt);
      const result = await openingBalanceApi.apply(slug, review.body);
      const record = result.openingBalance;
      if (!validRecord(record) || record.planHash !== review.body.planHash || record.cutOverDate !== review.body.input.cutOverDate || !Number.isSafeInteger(result.idempotency?.receiptId) || result.idempotency.receiptId <= 0 || typeof result.idempotency.replayed !== "boolean") throw new ApiError("invalid_opening_receipt", "Kvitteringen kunne ikke bekræftes. Kontrollér aktuel åbningsbalance før et nyt forsøg.", 200);
      confirmedReceipt = true;
      const canonical = await openingBalanceApi.read(slug);
      if (!sameRecord(record, canonical.openingBalance)) throw new ApiError("invalid_opening_readback", "Bogføringens aktuelle læsning kunne ikke bekræftes. Den oprindelige anmodning bevares.", 200);
      if (!alive.current) return;
      sessionStorage.removeItem(storageKey); marker.release(); setPending(null); setReview(null); setDone(record); setLines([blankLine(1), blankLine(2)]); setDate(""); setNote(""); setCitations([]); current.reload(); years.reload();
    } catch (cause) {
      if (!alive.current) return;
      const definite = !confirmedReceipt && cause instanceof ApiError && cause.status >= 400 && cause.status < 500 && ![408, 409].includes(cause.status) && cause.code !== "internal";
      if (definite) { sessionStorage.removeItem(storageKey); marker.release(); setPending(null); }
      setReconciled(false); setConfirmed(false); setError(cause instanceof Error ? cause.message : "Bogføringens udfald er uafklaret. Kontrollér status før et nyt forsøg.");
    } finally { working.current = false; if (alive.current) setBusy(false); }
  }
  const applyBlocked = busy || !confirmed || !caps.can("company.admin") || !caps.can("company.ledger.post") || (pending ? !reconciled : !review || review.plan.status !== "ready" || review.plan.blockers.length > 0 || !!review.plan.existing);
  return <section className="card" aria-label="Åbningsbalance"><h3>Åbningsbalance</h3><p>Angiv selv de saldi, virksomheden starter med på skæringsdatoen. Der bogføres højst én åbningsbalance. Tidligere arkivår er særskilt historik.</p>
    {error && !review && <p role="alert">{error}</p>}{current.error && <p role="alert">Åbningsbalancen kunne ikke læses: {current.error}</p>}{accounts.error && <p role="alert">Kontoplanen kunne ikke læses: {accounts.error}</p>}
    <Button variant="secondary" disabled={busy} onClick={() => { current.reload(); accounts.reload(); archives.reload(); years.reload(); }}>Genlæs åbningsbalance og kilder</Button>
    {validRecord(posted) && <RecordSummary slug={slug} record={posted} years={years.data ?? []} />}
    {pending && !review && <p role="alert">Et tidligere forsøg er uafklaret. Angiv præcis samme dato, linjer, note og arkivhenvisninger for at kontrollere forsøget. Ingen handling gentages automatisk.</p>}
    {(!posted || pending) && <><fieldset disabled={busy || Boolean(review)}><legend>Angiv åbningsbeløb</legend><label>Skæringsdato<Input aria-label="Skæringsdato" type="date" value={cutOverDate} onChange={event => setDate(event.target.value)} /></label><label>Note (valgfri)<Input aria-label="Note (valgfri)" value={note} onChange={event => setNote(event.target.value)} /></label><p>Beløb angives uden tusindtalsseparator med punktum og to decimaler, f.eks. 100.00. Serveren kontrollerer balance og bogføringsregler.</p>
      {lines.map((line, index) => <fieldset key={line.id}><legend>Åbningslinje {index + 1}</legend><label>Konto {index + 1}<Select aria-label={`Konto ${index + 1}`} value={line.accountNo} onChange={event => change(line.id, { accountNo: event.target.value })}><option value="">Vælg konto</option>{accounts.data?.accounts.filter(account => account.active !== false).map(account => <option key={account.accountNo} value={account.accountNo}>{account.accountNo} · {account.name}</option>)}</Select></label><label>Debet {index + 1}<Input aria-label={`Debet ${index + 1}`} inputMode="decimal" value={line.debit} onChange={event => change(line.id, { debit: event.target.value })} /></label><label>Kredit {index + 1}<Input aria-label={`Kredit ${index + 1}`} inputMode="decimal" value={line.credit} onChange={event => change(line.id, { credit: event.target.value })} /></label><label>Linjetekst {index + 1} (valgfri)<Input aria-label={`Linjetekst ${index + 1} (valgfri)`} value={line.text} onChange={event => change(line.id, { text: event.target.value })} /></label>{lines.length > 2 && <Button variant="secondary" onClick={() => setLines(old => old.filter(item => item.id !== line.id))}>Fjern linje {index + 1}</Button>}</fieldset>)}
      <Button variant="secondary" onClick={() => setLines(old => [...old, blankLine(next.current++)])}>Tilføj linje</Button><fieldset><legend>Arkivhenvisninger (valgfri)</legend><p>Vælg kun arkivimporter, der dokumenterer de beløb, du selv har angivet.</p>{archives.error && <p role="alert">Arkivhenvisninger kunne ikke hentes.</p>}{Array.from(new Map((archives.data?.years ?? []).map(year => [year.importId, year])).values()).map(year => <label key={year.importId}><Input type="checkbox" aria-label={`Arkivimport ${year.importId}`} checked={citations.includes(year.importId)} onChange={event => setCitations(old => event.target.checked ? [...old, year.importId] : old.filter(id => id !== year.importId))} />Arkivimport {year.importId} · {year.fiscalYear} · {year.declaredCurrency}<a href={`/api/companies/${encodeURIComponent(slug)}/archive-import/${year.importId}/original`} download>Hent arkivkilde {year.importId}</a></label>)}</fieldset></fieldset>
      <Button disabled={busy || Boolean(review) || current.loading || !!current.error || accounts.loading || !!accounts.error || marker.blocked && !pending} onClick={() => void makePlan()}>Gennemgå åbningsbalance</Button></>}
    {review && <Dialog title="Bekræft åbningsbalance" busy={busy} onClose={guard.onClose}>{guard.confirmation}{error && <p role="alert">{error}</p>}<PlanSummary review={review} />{pending && <><p role="alert">Uafklaret forsøg: behold den oprindelige anmodning og nøgle. Statuskontrol bogfører ikke.</p><Button variant="secondary" disabled={busy} onClick={() => void reconcile()}>Kontrollér aktuel åbningsbalance</Button>{reconciliation && <p role="status">{reconciliation}</p>}</>}<label><Input type="checkbox" checked={confirmed} disabled={busy} onChange={event => setConfirmed(event.target.checked)} />Jeg bekræfter skæringsdato, konti og åbningsbeløb</label><Button variant="secondary" disabled={busy || Boolean(pending)} onClick={guard.onClose}>Tilbage uden bogføring</Button><Button disabled={applyBlocked} onClick={() => void apply()}>{pending ? "Hent oprindelig kvittering / prøv samme anmodning" : "Bogfør åbningsbalance"}</Button></Dialog>}
  </section>;
}
