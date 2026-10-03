import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { Button, Dialog, Input, Select } from "./ui";
import { useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import type { CompanyPayables, DirectBankPayableCorrectionInput, DirectBankPayableCorrectionPlan } from "../lib/types";

type Props = { slug:string; payables:CompanyPayables; onApplied:()=>void; onClose:()=>void };

/** Two-step reviewed correction for a purchase that was booked directly to bank. */
export function DirectBankPayableCorrectionModal({slug,payables,onApplied,onClose: onDismiss}:Props) {
  const document=payables.unregisteredDocuments[0];
  const account=payables.expenseAccounts[0];
  const [documentId,setDocumentId]=useState(document?.id??0);
  const selected=useMemo(()=>payables.unregisteredDocuments.find(row=>row.id===documentId),[documentId,payables.unregisteredDocuments]);
  const [bankTransactionId,setBankTransactionId]=useState("");
  const [billDate,setBillDate]=useState(document?.invoiceDate??"");
  const [dueDate,setDueDate]=useState(document?.invoiceDate??"");
  const [expenseAccountNo,setExpenseAccountNo]=useState(account?.accountNo??"");
  const [reason,setReason]=useState("Ret direkte bankkøb til kreditorforløb");
  const [plan,setPlan]=useState<DirectBankPayableCorrectionPlan|null>(null);
  const [busy,setBusy]=useState(false); const [error,setError]=useState<string|null>(null);
  const dirty = Boolean(bankTransactionId || plan) || documentId !== (document?.id ?? 0) || dueDate !== (document?.invoiceDate ?? "") || expenseAccountNo !== (account?.accountNo ?? "") || reason !== "Ret direkte bankkøb til kreditorforløb";
  const outcome = useMutationOutcome(onApplied);
  const guard = useDiscardGuard(dirty, onDismiss);
  const { onClose } = guard;
  const correctionKey = useRef<string>();
  const input=():DirectBankPayableCorrectionInput=>({documentId,bankTransactionId:Number(bankTransactionId),billDate,dueDate,expenseAccountNo,vatTreatment:"standard"});
  const review=async()=>{setBusy(true);setError(null);try{setPlan(await api.planDirectBankPayableCorrection(slug,input()));}catch(e){setError(e instanceof Error?e.message:"Planen kunne ikke oprettes");}finally{setBusy(false);}};
  const apply=async()=>{if (outcome.isBlocked()) return;if(!plan)return;if (!correctionKey.current) correctionKey.current = crypto.randomUUID();setBusy(true);setError(null);try{await outcome.run(() => api.applyDirectBankPayableCorrection(slug,{...input(),planHash:plan.planHash,reason,idempotencyKey:correctionKey.current!}));onApplied();guard.dismiss();}catch(e){setError(e instanceof Error?e.message:"Korrektionen kunne ikke gennemføres");}finally{setBusy(false);}};
  return <Dialog title="Ret direkte bankkøb" onClose={onClose} busy={busy}>
    {outcome.feedback}
      {guard.confirmation}
    <div className="page-head"><div><p className="muted">Bevar fakturadatoen, opret kreditorposten og flyt bankafregningen til bankdatoen.</p></div><Button variant="secondary" className="btn secondary" type="button" onClick={onClose}>Luk</Button></div>
    <div className="form-grid">
      <label>Bilag<Select disabled={outcome.blocked} value={documentId} onChange={event=>{const id=Number(event.target.value);setDocumentId(id);setBillDate(payables.unregisteredDocuments.find(row=>row.id===id)?.invoiceDate??"");setPlan(null);}}>{payables.unregisteredDocuments.map(row=><option key={row.id} value={row.id}>{row.invoiceNo??row.documentNo??`Bilag ${row.id}`}</option>)}</Select></label>
      <label>Banktransaktions-id<Input disabled={outcome.blocked} inputMode="numeric" value={bankTransactionId} onChange={event=>{setBankTransactionId(event.target.value);setPlan(null);}} /></label>
      <label>Fakturadato<Input disabled={outcome.blocked} type="date" value={billDate} readOnly aria-describedby="invoice-date-note" /></label>
      <label>Forfaldsdato<Input disabled={outcome.blocked} type="date" value={dueDate} onChange={event=>{setDueDate(event.target.value);setPlan(null);}} /></label>
      <label>Udgiftskonto<Select disabled={outcome.blocked} value={expenseAccountNo} onChange={event=>{setExpenseAccountNo(event.target.value);setPlan(null);}}>{payables.expenseAccounts.map(row=><option key={row.accountNo} value={row.accountNo}>{row.accountNo} · {row.name}</option>)}</Select></label>
      <label>Begrundelse<Input disabled={outcome.blocked} value={reason} onChange={event=>setReason(event.target.value)} /></label>
    </div>
    <p id="invoice-date-note" className="muted">Fakturadatoen kommer uændret fra bilaget {selected?.invoiceNo??""}.</p>
    {error&&<p className="error" role="alert">{error}</p>}
    {plan&&<div className="card"><strong>Kontrollér planen</strong><p className="muted">Bankdato {plan.bankDate} · {plan.bankAmount.toFixed(2)} DKK · plan {plan.planHash.slice(0,12)}…</p></div>}
    <div className="row-actions"><Button variant="secondary" className="btn secondary" type="button" disabled={busy||!documentId||!Number(bankTransactionId)||!billDate||!dueDate||!expenseAccountNo} onClick={review}>{busy?"Arbejder…":"Opret plan"}</Button><Button requiredPermission="company.ledger.post" variant="danger" className="btn danger" type="button" disabled={outcome.blocked || (busy||!plan||!reason.trim())} onClick={apply}>Bekræft korrektion</Button></div>
  </Dialog>;
}
