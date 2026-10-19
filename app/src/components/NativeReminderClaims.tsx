import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { formatKroner, parseDanishAmount } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import { useCapabilities } from "../lib/useCapabilities";
import { ConfirmDialog } from "./ConfirmDialog";
import { Banner } from "./Feedback";
import { Button, Input } from "./ui";

/** Separate registration, posting and actual bank receipt. No sending or automatic action. */
export function NativeReminderClaims({slug,id,onChanged,onClose}:{slug:string;id:number;onChanged:()=>void;onClose:()=>void}){
 const {can}=useCapabilities(slug);
 const read=useAsync(()=>api.invoiceReminderClaims(slug,id),[slug,id]);
 const status=read.error?null:read.data,error=read.error;
 const [date,setDate]=useState(""),[fee,setFee]=useState(""),[reminder,setReminder]=useState(""),[postDate,setPostDate]=useState(""),[receivable,setReceivable]=useState(""),[income,setIncome]=useState(""),[bank,setBank]=useState("");
 const [action,setAction]=useState<"register"|"post"|"settle"|null>(null);
 useEffect(()=>{if(status){setDate(d=>d||status.asOfDate);setPostDate(d=>d||status.asOfDate);setFee(f=>f||String(status.rules.maximumFeeAmount));}},[status]);
 const refresh=()=>{read.reload();onChanged();};
 const pending=status?.reminders.filter(r=>r.journalEntryId===null)??[], selected=pending.find(r=>r.reminderId===Number(reminder)), receipt=status?.incomingBankTransactions.find(r=>r.id===Number(bank));
 return <section className="card" aria-label="Rykkergebyrer og krav">
  <h3>Rykkergebyrer og krav</h3><p className="muted">Registrering, særskilt bogføring og afstemning af en faktisk indbetaling. Der sendes ingen e-mail eller rykker automatisk. Fakturaens hovedstol skal være betalt før indbetaling på gebyrkravet.</p>
  <Button type="button" variant="secondary" onClick={onClose}>Luk krav</Button>
  {error&&<Banner kind="error">{error}</Banner>}
  {!status&&!error&&<p>Henter aktuelle krav og kildebeviser…</p>}
  {status&&<>
   <p>Faktura {status.invoiceNumber} · hovedstol {formatKroner(status.principalOpenBalance,"DKK")} · registrerede gebyrer {formatKroner(status.totalReminderFees,"DKK")} · bogført restkrav {formatKroner(status.ledgerBackedClaimBalance,"DKK")}</p>
   <ul>{status.reminders.map(r=><li key={r.reminderId}>{r.reminderDate}: {formatKroner(r.feeAmount,"DKK")} — {r.journalEntryId===null?"Ikke bogført":`Bogført i postering ${r.journalEntryId}`}</li>)}</ul>
   <ul>{status.claimPayments.map(p=><li key={p.claimPaymentId}>Indbetaling {p.paymentDate}: {formatKroner(p.amount,"DKK")} · bankpost {p.bankTransactionId} · postering {p.journalEntryId}</li>)}</ul>
   <p>Eksisterende regel {status.rules.ruleId}: højst {status.rules.maximumFeeAmount} kr., højst {status.rules.maximumReminders} gebyrer og mindst {status.rules.minimumDaysBetweenReminders} dage mellem registreringer. Serveren kontrollerer forfald og aktuelle saldi igen.</p>
   {can("company.draft.write")&&<div>
    <label>Rykkerdato<Input type="date" value={date} onChange={e=>setDate(e.target.value)}/></label>
    <label>Rykkergebyr i DKK<Input value={fee} inputMode="decimal" onChange={e=>setFee(e.target.value)}/></label>
    <Button type="button" onClick={()=>setAction("register")}>Gennemgå registrering af gebyr</Button>
   </div>}
   {can("company.ledger.post")&&pending.length>0&&<div>
    <label>Registreret gebyr<select value={reminder} onChange={e=>setReminder(e.target.value)}><option value="">Vælg gebyr</option>{pending.map(r=><option key={r.reminderId} value={r.reminderId}>{r.reminderDate} · {formatKroner(r.feeAmount,"DKK")}</option>)}</select></label>
    <label>Bogføringsdato for gebyr<Input type="date" value={postDate} onChange={e=>setPostDate(e.target.value)}/></label>
    <label>Konto til gebyrtilgodehavende<select value={receivable} onChange={e=>setReceivable(e.target.value)}><option value="">Vælg konto</option>{status.accounts.filter(a=>a.type==="asset").map(a=><option key={a.accountNo} value={a.accountNo}>{a.accountNo} · {a.name}</option>)}</select></label>
    <label>Konto til gebyrindtægt<select value={income} onChange={e=>setIncome(e.target.value)}><option value="">Vælg konto</option>{status.accounts.filter(a=>a.type==="income").map(a=><option key={a.accountNo} value={a.accountNo}>{a.accountNo} · {a.name}</option>)}</select></label>
    <Button type="button" disabled={!selected||!receivable||!income} onClick={()=>setAction("post")}>Gennemgå særskilt bogføring</Button>
   </div>}
   {can("company.ledger.post")&&<div>
    <label>Faktisk indbetaling på gebyrkrav<select value={bank} onChange={e=>setBank(e.target.value)}><option value="">Vælg indgående bankpost</option>{status.incomingBankTransactions.map(b=><option key={b.id} value={b.id}>{b.date} · {b.text} · {formatKroner(b.amount,"DKK")} · {b.reference}</option>)}</select></label>
    {receipt&&<p>Bankkonto {receipt.bankAccountNo ?? "Ikke tilgængelig"} · kildehash {receipt.sourceHash ?? "Ikke tilgængelig"} · transaktionshash {receipt.transactionHash ?? "Ikke tilgængelig"}</p>}
    {!status.settlementAvailable&&<p>Afstem først hovedstolen via fakturaens «Afstem», og bogfør alle registrerede gebyrer. Hent derefter opdateret kravstatus.</p>}
    <Button type="button" variant="secondary" onClick={refresh}>Opdatér kravstatus</Button>
    <Button type="button" disabled={!status.settlementAvailable||!receipt?.bankAccountNo||!receipt.sourceHash||!receipt.transactionHash} onClick={()=>setAction("settle")}>Gennemgå indbetaling på gebyrkrav</Button>
   </div>}
  </>}
  {action&&status&&<ConfirmDialog key={`${id}:${action}`} operationKey={`reminder-claims:${slug}:${id}:${action}`}
   title={action==="register"?"Registrér rykkergebyr":action==="post"?"Bogfør registreret rykkergebyr":"Afstem indbetaling på gebyrkrav"}
   body={<p>{action==="register"?`Registrér ${fee} DKK på dato ${date} for ${status.invoiceNumber}. Ingen bogføring eller e-mail i dette trin.`:action==="post"?`Bogfør gebyr ${selected?.reminderId}: ${selected?.feeAmount} DKK fra konto ${receivable} til gebyrindtægt ${income}. Dato: ${postDate}. Ingen moms.`:`Bogfør hele den faktiske indbetaling ${receipt?.amount} DKK på ${receipt?.date}, bankpost ${receipt?.id}, konto ${receipt?.bankAccountNo}, mod kravets historiske debitorkonti. Ingen ny indtægt eller moms.`}</p>}
   confirmLabel={action==="register"?"Registrér gebyr":action==="post"?"Bogfør gebyr":"Afstem gebyrindbetaling"} confirmKind="danger" noteLabel={action==="register"?"Note":undefined}
   onConfirm={async note=>{
    if(action==="register"){const amount=parseDanishAmount(fee);if(amount===null||amount<=0)throw new Error("Angiv et positivt gebyrbeløb.");await api.registerNativeReminder(slug,{invoiceDocumentId:id,reminderDate:date,feeAmount:amount,note,confirm:true});}
    else if(action==="post"){if(!selected)throw new Error("Hent det aktuelle registrerede gebyr igen.");await api.postNativeReminder(slug,{invoiceDocumentId:id,reminderId:selected.reminderId,transactionDate:postDate,receivableAccountNo:receivable,reminderIncomeAccountNo:income,confirm:true});}
    else{if(!receipt)throw new Error("Vælg en faktisk bankpost.");await api.settleNativeInvoiceClaims(slug,{invoiceDocumentId:id,bankTransactionId:receipt.id,confirm:true});}
    refresh();
   }} onRefresh={refresh} onClose={()=>setAction(null)}/>}
 </section>;
}
