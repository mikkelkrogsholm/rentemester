import { useMemo, useState } from "react";
import { api } from "../lib/api";
import type { CompanyPayables, DirectBankPayableCorrectionInput, DirectBankPayableCorrectionPlan } from "../lib/types";

type Props = { slug:string; payables:CompanyPayables; onApplied:()=>void; onClose:()=>void };

/** Two-step reviewed correction for a purchase that was booked directly to bank. */
export function DirectBankPayableCorrectionModal({slug,payables,onApplied,onClose}:Props) {
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
  const input=():DirectBankPayableCorrectionInput=>({documentId,bankTransactionId:Number(bankTransactionId),billDate,dueDate,expenseAccountNo,vatTreatment:"standard"});
  const review=async()=>{setBusy(true);setError(null);try{setPlan(await api.planDirectBankPayableCorrection(slug,input()));}catch(e){setError(e instanceof Error?e.message:"Planen kunne ikke oprettes");}finally{setBusy(false);}};
  const apply=async()=>{if(!plan)return;setBusy(true);setError(null);try{await api.applyDirectBankPayableCorrection(slug,{...input(),planHash:plan.planHash,reason,idempotencyKey:crypto.randomUUID()});onApplied();onClose();}catch(e){setError(e instanceof Error?e.message:"Korrektionen kunne ikke gennemføres");}finally{setBusy(false);}};
  return <div className="modal-backdrop" role="presentation"><section className="modal card" role="dialog" aria-modal="true" aria-labelledby="direct-payable-title">
    <div className="page-head"><div><h3 id="direct-payable-title">Ret direkte bankkøb</h3><p className="muted">Bevar fakturadatoen, opret kreditorposten og flyt bankafregningen til bankdatoen.</p></div><button className="btn secondary" type="button" onClick={onClose}>Luk</button></div>
    <div className="form-grid">
      <label>Bilag<select value={documentId} onChange={event=>{const id=Number(event.target.value);setDocumentId(id);setBillDate(payables.unregisteredDocuments.find(row=>row.id===id)?.invoiceDate??"");setPlan(null);}}>{payables.unregisteredDocuments.map(row=><option key={row.id} value={row.id}>{row.invoiceNo??row.documentNo??`Bilag ${row.id}`}</option>)}</select></label>
      <label>Banktransaktions-id<input inputMode="numeric" value={bankTransactionId} onChange={event=>{setBankTransactionId(event.target.value);setPlan(null);}} /></label>
      <label>Fakturadato<input type="date" value={billDate} readOnly aria-describedby="invoice-date-note" /></label>
      <label>Forfaldsdato<input type="date" value={dueDate} onChange={event=>{setDueDate(event.target.value);setPlan(null);}} /></label>
      <label>Udgiftskonto<select value={expenseAccountNo} onChange={event=>{setExpenseAccountNo(event.target.value);setPlan(null);}}>{payables.expenseAccounts.map(row=><option key={row.accountNo} value={row.accountNo}>{row.accountNo} · {row.name}</option>)}</select></label>
      <label>Begrundelse<input value={reason} onChange={event=>setReason(event.target.value)} /></label>
    </div>
    <p id="invoice-date-note" className="muted">Fakturadatoen kommer uændret fra bilaget {selected?.invoiceNo??""}.</p>
    {error&&<p className="error" role="alert">{error}</p>}
    {plan&&<div className="card"><strong>Kontrollér planen</strong><p className="muted">Bankdato {plan.bankDate} · {plan.bankAmount.toFixed(2)} DKK · plan {plan.planHash.slice(0,12)}…</p></div>}
    <div className="row-actions"><button className="btn secondary" type="button" disabled={busy||!documentId||!Number(bankTransactionId)||!billDate||!dueDate||!expenseAccountNo} onClick={review}>{busy?"Arbejder…":"Opret plan"}</button><button className="btn danger" type="button" disabled={busy||!plan||!reason.trim()} onClick={apply}>Bekræft korrektion</button></div>
  </section></div>;
}
