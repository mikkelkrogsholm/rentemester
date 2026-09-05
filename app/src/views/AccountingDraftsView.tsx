import { useState, type FormEvent } from "react";
import { Link, useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import type { AccountingDraft, AccountingDraftLine, AccountingDraftPayload } from "../lib/types";
import { useAsync } from "../lib/useAsync";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";
import { formatKroner, todayIso } from "../lib/format";

type LineForm = { accountNo: string; debitAmount: string; creditAmount: string; vatCode: string; text: string };
const EMPTY_LINE: LineForm = { accountNo: "", debitAmount: "", creditAmount: "", vatCode: "", text: "" };
const STATUS_LABEL: Record<AccountingDraft["status"], string> = {
  created: "Kladde",
  revised: "Revideret",
  submitted: "Afventer godkendelse",
  rejected: "Afvist",
  approved_posted: "Godkendt og bogført",
};

function optionalPositiveInteger(value: string): number | undefined {
  if (!value.trim()) return undefined;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : Number.NaN;
}

function amount(value: string): number | undefined {
  if (!value.trim()) return undefined;
  const parsed = Number(value.replace(",", "."));
  return Number.isFinite(parsed) && parsed > 0 ? parsed : Number.NaN;
}

function toPayload(input: { date: string; text: string; documentId: string; bankId: string; lines: LineForm[] }): AccountingDraftPayload {
  const documentId = optionalPositiveInteger(input.documentId);
  const sourceBankTransactionId = optionalPositiveInteger(input.bankId);
  if (Number.isNaN(documentId) || Number.isNaN(sourceBankTransactionId)) throw new Error("Bilags- og bank-id skal være positive heltal.");
  const lines: AccountingDraftLine[] = input.lines.map((line, index) => {
    const debitAmount = amount(line.debitAmount);
    const creditAmount = amount(line.creditAmount);
    if (!line.accountNo.trim()) throw new Error(`Linje ${index + 1}: angiv konto.`);
    if (Number.isNaN(debitAmount) || Number.isNaN(creditAmount) || (debitAmount === undefined) === (creditAmount === undefined)) {
      throw new Error(`Linje ${index + 1}: angiv enten et positivt debet- eller kreditbeløb.`);
    }
    return {
      accountNo: line.accountNo.trim(),
      ...(debitAmount === undefined ? {} : { debitAmount }),
      ...(creditAmount === undefined ? {} : { creditAmount }),
      ...(line.vatCode.trim() ? { vatCode: line.vatCode.trim() } : {}),
      ...(line.text.trim() ? { text: line.text.trim() } : {}),
    };
  });
  return {
    transactionDate: input.date,
    text: input.text.trim(),
    ...(documentId === undefined ? {} : { documentId }),
    ...(sourceBankTransactionId === undefined ? {} : { sourceBankTransactionId }),
    lines,
  };
}

export function AccountingDraftsView() {
  const { slug = "" } = useParams();
  const state = useAsync(() => api.accountingDrafts(slug), [slug]);
  const policy = useAsync(() => api.accountingApprovalPolicy(slug), [slug]);
  const [draftId, setDraftId] = useState("");
  const [date, setDate] = useState(todayIso());
  const [text, setText] = useState("");
  const [documentId, setDocumentId] = useState("");
  const [bankId, setBankId] = useState("");
  const [lines, setLines] = useState<LineForm[]>([{ ...EMPTY_LINE }, { ...EMPTY_LINE }]);
  const [editing, setEditing] = useState<AccountingDraft | null>(null);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [pending, setPending] = useState<{ kind: "submit" | "reject" | "approve"; draft: AccountingDraft } | null>(null);

  async function create(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setActionError(null);
    try {
      const payload = toPayload({ date, text, documentId, bankId, lines });
      if (editing) await api.reviseAccountingDraft(slug, editing, payload);
      else await api.createAccountingDraft(slug, draftId, payload);
      setDraftId(""); setText(""); setDocumentId(""); setBankId("");
      setLines([{ ...EMPTY_LINE }, { ...EMPTY_LINE }]);
      setEditing(null);
      state.reload();
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "Kladde kunne ikke oprettes.");
    } finally {
      setBusy(false);
    }
  }

  function edit(draft: AccountingDraft) {
    setEditing(draft);
    setDraftId(draft.id);
    setDate(draft.payload.transactionDate);
    setText(draft.payload.text);
    setDocumentId(draft.payload.documentId?.toString() ?? "");
    setBankId(draft.payload.sourceBankTransactionId?.toString() ?? "");
    setLines(draft.payload.lines.map((line) => ({
      accountNo: line.accountNo,
      debitAmount: line.debitAmount?.toString() ?? "",
      creditAmount: line.creditAmount?.toString() ?? "",
      vatCode: line.vatCode ?? "",
      text: line.text ?? "",
    })));
    window.scrollTo?.({ top: 0, behavior: "smooth" });
  }

  async function action(kind: "submit" | "reject" | "approve", draft: AccountingDraft, note: string) {
    setActionError(null);
    try {
      if (kind === "submit") await api.submitAccountingDraft(slug, draft);
      else if (kind === "reject") {
        if (!note.trim()) throw new Error("En afvisning kræver en begrundelse.");
        await api.rejectAccountingDraft(slug, draft, note, policy.data?.eventHash);
      } else await api.approveAndPostAccountingDraft(slug, draft, policy.data?.eventHash);
      state.reload();
    } catch (error) {
      setActionError(error instanceof ApiError || error instanceof Error ? error.message : "Handlingen kunne ikke gennemføres.");
      throw error;
    }
  }

  if (state.loading && !state.data) return <Loading label="Henter bogføringskladder…" />;
  if (state.error || policy.error) return <ErrorState message={state.error ?? policy.error ?? "Kladder kunne ikke hentes."} onRetry={() => { state.reload(); policy.reload(); }} />;

  return <section className="statement accounting-drafts-view">
    <div className="page-head"><div><h2>Bogføringskladder</h2><p className="muted">Kladde → indsendelse → uafhængig godkendelse → atomisk bogføring</p></div><Link className="btn secondary" to={`/companies/${slug}/posteringer`}>Se bogførte posteringer</Link></div>
    <p className="muted">Den indsendte version låses med en SHA-256-identitet. Godkenderen skal være en anden bruger end forfatteren og indsenderen.</p>
    {actionError && <div className="card archived-notice" role="alert"><p>{actionError}</p></div>}

    <form className="card accounting-draft-form" onSubmit={create}>
      <h3>{editing ? `Ny version af ${editing.id}` : "Ny kladde"}</h3>
      <div className="form-grid">
        <label>Kladde-id<input required disabled={editing !== null} pattern="[a-z][a-z0-9-]{0,63}" value={draftId} onChange={(event) => setDraftId(event.target.value.toLowerCase())} placeholder="fx bank-2026-08-001" /></label>
        <label>Dato<input required type="date" value={date} onChange={(event) => setDate(event.target.value)} /></label>
        <label className="full-width">Tekst<input required value={text} onChange={(event) => setText(event.target.value)} /></label>
        <label>Bilags-id (valgfrit)<input inputMode="numeric" value={documentId} onChange={(event) => setDocumentId(event.target.value)} /></label>
        <label>Bankpost-id (valgfrit)<input inputMode="numeric" value={bankId} onChange={(event) => setBankId(event.target.value)} /></label>
      </div>
      <div className="table-scroll"><table className="data"><thead><tr><th>Konto</th><th>Debet</th><th>Kredit</th><th>Momskode</th><th>Linjetekst</th><th /></tr></thead><tbody>{lines.map((line, index) => <tr key={index}>
        <td><input aria-label={`Konto linje ${index + 1}`} value={line.accountNo} onChange={(event) => setLines((current) => current.map((item, i) => i === index ? { ...item, accountNo: event.target.value } : item))} /></td>
        <td><input aria-label={`Debet linje ${index + 1}`} inputMode="decimal" value={line.debitAmount} onChange={(event) => setLines((current) => current.map((item, i) => i === index ? { ...item, debitAmount: event.target.value, creditAmount: event.target.value ? "" : item.creditAmount } : item))} /></td>
        <td><input aria-label={`Kredit linje ${index + 1}`} inputMode="decimal" value={line.creditAmount} onChange={(event) => setLines((current) => current.map((item, i) => i === index ? { ...item, creditAmount: event.target.value, debitAmount: event.target.value ? "" : item.debitAmount } : item))} /></td>
        <td><input aria-label={`Momskode linje ${index + 1}`} value={line.vatCode} onChange={(event) => setLines((current) => current.map((item, i) => i === index ? { ...item, vatCode: event.target.value } : item))} /></td>
        <td><input aria-label={`Tekst linje ${index + 1}`} value={line.text} onChange={(event) => setLines((current) => current.map((item, i) => i === index ? { ...item, text: event.target.value } : item))} /></td>
        <td><button type="button" className="btn secondary" disabled={lines.length <= 2} onClick={() => setLines((current) => current.filter((_, i) => i !== index))}>Fjern</button></td>
      </tr>)}</tbody></table></div>
      <div className="row-actions"><button type="button" className="btn secondary" onClick={() => setLines((current) => [...current, { ...EMPTY_LINE }])}>Tilføj linje</button>{editing && <button type="button" className="btn secondary" onClick={() => { setEditing(null); setDraftId(""); setText(""); setDocumentId(""); setBankId(""); setLines([{ ...EMPTY_LINE }, { ...EMPTY_LINE }]); }}>Annullér revision</button>}<button className="btn" type="submit" disabled={busy}>{busy ? "Gemmer…" : editing ? "Gem ny version" : "Opret kladde"}</button></div>
    </form>

    <section className="card"><h3>Kladder ({state.data!.length})</h3>{state.data!.length === 0 ? <p className="muted">Ingen kladder endnu.</p> : <div className="table-scroll"><table className="data"><thead><tr><th>Id / version</th><th>Status</th><th>Dato og tekst</th><th>Beløb</th><th>Evidens</th><th>Handling</th></tr></thead><tbody>{state.data!.map((draft) => <tr key={draft.id}>
      <td><strong>{draft.id}</strong><div className="muted">v{draft.version}</div></td><td><span className={`pill ${draft.status === "approved_posted" ? "ok" : draft.status === "rejected" ? "warn" : ""}`}>{STATUS_LABEL[draft.status]}</span>{draft.reason && <div className="muted">{draft.reason}</div>}</td>
      <td>{draft.payload.transactionDate}<div className="muted">{draft.payload.text}</div></td><td className="num">{formatKroner(draft.payload.lines.reduce((sum, line) => sum + (line.debitAmount ?? 0), 0))}</td>
      <td><code title={draft.eventHash}>{draft.eventHash.slice(0, 12)}…</code>{draft.journalEntryId && <div className="muted">Journal #{draft.journalEntryId}</div>}</td>
      <td><div className="row-actions">{(draft.status === "created" || draft.status === "revised") && <><button className="btn" type="button" onClick={() => setPending({ kind: "submit", draft })}>Indsend</button><button className="btn secondary" type="button" onClick={() => edit(draft)}>Redigér ny version</button></>}{draft.status === "rejected" && <button className="btn secondary" type="button" onClick={() => edit(draft)}>Ret og opret ny version</button>}{draft.status === "submitted" && <><button className="btn" type="button" onClick={() => setPending({ kind: "approve", draft })}>Godkend og bogfør</button><button className="btn secondary" type="button" onClick={() => setPending({ kind: "reject", draft })}>Afvis</button></>}</div></td>
    </tr>)}</tbody></table></div>}</section>

    {pending && <ConfirmDialog title={pending.kind === "approve" ? "Godkend og bogfør" : pending.kind === "reject" ? "Afvis kladde" : "Indsend kladde"} body={<p>{pending.kind === "approve" ? "Den præcise indsendte version bogføres irreversibelt og audit-logges." : pending.kind === "reject" ? "Kladdeversionen afvises; skriv hvorfor." : "Den aktuelle version låses til uafhængigt review."}</p>} confirmLabel={pending.kind === "approve" ? "Godkend og bogfør" : pending.kind === "reject" ? "Afvis" : "Indsend"} confirmKind={pending.kind === "reject" ? "danger" : "primary"} noteLabel={pending.kind === "reject" ? "Begrundelse" : undefined} onConfirm={(note) => action(pending.kind, pending.draft, note)} onClose={() => setPending(null)} />}
  </section>;
}
