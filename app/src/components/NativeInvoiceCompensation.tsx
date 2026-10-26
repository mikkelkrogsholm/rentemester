import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { api } from "../lib/api";
import type { NativeInvoiceCompensationCalculate, NativeInvoiceCompensationResult, NativeReminderClaimStatus } from "../lib/api/invoices";
import { formatKroner, parseDanishAmount } from "../lib/format";
import { useCapabilities } from "../lib/useCapabilities";
import { ConfirmDialog } from "./ConfirmDialog";
import { Banner } from "./Feedback";
import { Button, Input } from "./ui";

type Props={slug:string;id:number;asOfDate:string;accounts:NativeReminderClaimStatus["accounts"];onChanged:()=>void};
/** Calculation, registration and posting are separate explicit human actions. */
export function NativeInvoiceCompensation({slug,id,asOfDate,accounts,onChanged}:Props) {
 const {can}=useCapabilities(slug);
 const [date,setDate]=useState(asOfDate),[amount,setAmount]=useState("");
 const [result,setResult]=useState<NativeInvoiceCompensationResult|null>(null),[error,setError]=useState<string|null>(null),[busy,setBusy]=useState(false);
 const [action,setAction]=useState<"register"|"post"|null>(null);
 const [postDate,setPostDate]=useState(asOfDate),[receivable,setReceivable]=useState(""),[income,setIncome]=useState("");
 async function calculate(input?:NativeInvoiceCompensationCalculate) {
  setBusy(true);setError(null);setResult(null);
  try {
   const parsed=amount===""?undefined:parseDanishAmount(amount);
   if(!input&&(!date||parsed===null||(parsed!==undefined&&parsed<=0)))throw new Error("Angiv en dato og et positivt kompensationsbeløb, eller lad beløbet stå tomt for at bruge serverens regel.");
   const next=await api.calculateNativeInvoiceCompensation(slug,input??{invoiceDocumentId:id,asOfDate:date,...(parsed!==undefined&&parsed!==null?{compensationAmountDkk:parsed}:{})});
   setResult(next);
  } catch(e) {setError(e instanceof Error?e.message:"Kompensationskravet kunne ikke hentes.");}
  finally {setBusy(false);}
 }
 const existing=result?.claimId!==null&&result?.claimId!==undefined;
 const posted=result?.postedJournalEntryId!==null&&result?.postedJournalEntryId!==undefined;
 return <section aria-label="Kompensationskrav">
  <h4>Kompensationskrav</h4>
  <p {...stylex.props(cockpitStyles.muted)}>Beregn først, om fakturaen kan bære et kompensationskrav. Registrering opretter kun kravet; bogføring sker i et særskilt trin. Der sendes ingen e-mail.</p>
  <label>Opgørelsesdato for kompensation<Input type="date" value={date} disabled={busy} onChange={e=>{setDate(e.target.value);setResult(null);}}/></label>
  <label>Kompensation i DKK<Input inputMode="decimal" value={amount} disabled={busy} placeholder="Brug serverens regel" onChange={e=>{setAmount(e.target.value);setResult(null);}}/></label>
  <Button type="button" variant="secondary" disabled={busy} onClick={()=>void calculate()}>{busy?"Henter kompensation…":"Beregn kompensation"}</Button>
  {error&&<Banner kind="error">{error}</Banner>}
  {result&&<>
   <p>Faktura {result.invoiceNumber} · hovedstol {formatKroner(result.principalOpenBalance,result.principalCurrency)} · forfald {result.effectiveDueDate} · {result.overdueDays} dage over forfald.</p>
   <p>Kompensation: {formatKroner(result.compensationAmountDkk,"DKK")}. {result.eligible?"Kan registreres efter bekræftelse.":result.reason??"Kan ikke registreres på det aktuelle grundlag."}</p>
   {result.errors.length>0&&<Banner kind="error">{result.errors.join(" ")}</Banner>}
   {existing&&<p>Registreret krav {result.claimId} den {result.claimDate}: {formatKroner(result.registeredAmountDkk ?? result.compensationAmountDkk,"DKK")} · {posted?`Bogført i ${result.postedJournalEntryNo ?? result.postedJournalEntryId}`:"Ikke bogført"}.</p>}
   {!existing&&result.eligible&&can("company.draft.write")&&<Button type="button" onClick={()=>setAction("register")}>Gennemgå registrering af kompensation</Button>}
   {existing&&!posted&&can("company.ledger.post")&&<div>
    <label>Bogføringsdato for kompensation<Input type="date" min={result.claimDate??undefined} value={postDate} onChange={e=>setPostDate(e.target.value)}/></label>
    <label>Konto til kompensationstilgodehavende<select aria-label="Konto til kompensationstilgodehavende" value={receivable} onChange={e=>setReceivable(e.target.value)}><option value="">Vælg aktivkonto</option>{accounts.filter(a=>a.type==="asset"&&a.normalBalance==="debit").map(a=><option key={a.accountNo} value={a.accountNo}>{a.accountNo} · {a.name}</option>)}</select></label>
    <label>Konto til kompensationsindtægt<select aria-label="Konto til kompensationsindtægt" value={income} onChange={e=>setIncome(e.target.value)}><option value="">Vælg indtægtskonto</option>{accounts.filter(a=>a.type==="income"&&a.normalBalance==="credit").map(a=><option key={a.accountNo} value={a.accountNo}>{a.accountNo} · {a.name}</option>)}</select></label>
    <Button type="button" disabled={!receivable||!income||!postDate} onClick={()=>setAction("post")}>Gennemgå bogføring af kompensation</Button>
   </div>}
   {existing&&<p {...stylex.props(cockpitStyles.muted)}>En faktisk indbetaling på kravet afstemmes i DKK i «Faktisk indbetaling på gebyrkrav», efter at hovedstolen er betalt og alle krav er bogført.</p>}
  </>}
  {action&&result&&<ConfirmDialog key={`${id}:${action}`} operationKey={`compensation:${slug}:${id}:${action}`}
   title={action==="register"?"Registrér kompensationskrav":"Bogfør kompensationskrav"}
   body={<p>{action==="register"?`Registrér ${result.compensationAmountDkk} DKK for ${result.invoiceNumber} pr. ${result.asOfDate}. Ingen journalpostering i dette trin.`:`Bogfør krav ${result.claimId}: ${result.registeredAmountDkk} DKK på ${postDate}, debet tilgodehavende ${receivable}, kredit kompensationsindtægt ${income}. Ingen moms.`}</p>}
   confirmLabel={action==="register"?"Registrér kompensation":"Bogfør kompensation"} confirmKind="danger" noteLabel={action==="register"?"Note":undefined}
   onConfirm={async note=>{
    if(action==="register")await api.registerNativeInvoiceCompensation(slug,{invoiceDocumentId:id,asOfDate:result.asOfDate,compensationAmountDkk:result.compensationAmountDkk,note,confirm:true});
    else await api.postNativeInvoiceCompensation(slug,{invoiceDocumentId:id,transactionDate:postDate,receivableAccountNo:receivable,compensationIncomeAccountNo:income,confirm:true});
    await calculate({invoiceDocumentId:id,asOfDate:result.asOfDate,compensationAmountDkk:result.compensationAmountDkk});onChanged();
   }} onRefresh={()=>calculate()} onClose={()=>setAction(null)}/>}
 </section>;
}
