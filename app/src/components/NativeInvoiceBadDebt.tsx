import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { NativeInvoiceBadDebtInput, NativeInvoiceBadDebtReceipt, NativeReminderClaimStatus } from "../lib/api/invoices";
import { formatKroner, parseDanishAmount } from "../lib/format";
import { useCapabilities } from "../lib/useCapabilities";
import { ConfirmDialog } from "./ConfirmDialog";
import { Banner } from "./Feedback";
import { Button, Input } from "./ui";

type Props={slug:string;id:number;status:NativeReminderClaimStatus;onChanged:()=>void};
type Review={body:NativeInvoiceBadDebtInput;currency:string;invoiceNumber:string;openPrincipal:number};
/** Canonical reads never repeat a write or release an uncertain operation. */
export function NativeInvoiceBadDebt({slug,id,status,onChanged}:Props){
 const {can}=useCapabilities(slug);
 const [current,setCurrent]=useState<NativeReminderClaimStatus|null>(status),[date,setDate]=useState(status.asOfDate),[amount,setAmount]=useState(""),[account,setAccount]=useState(""),[note,setNote]=useState(""),[established,setEstablished]=useState(false),[review,setReview]=useState<Review|null>(null),[receipt,setReceipt]=useState<NativeInvoiceBadDebtReceipt|null>(null),[error,setError]=useState<string|null>(null),[busy,setBusy]=useState(false);
 useEffect(()=>setCurrent(status),[status]);
 const parsed=amount.trim()===""?undefined:parseDanishAmount(amount);
 const selected=current?.accounts.find(a=>a.accountNo===account&&a.type==="expense"&&a.normalBalance==="debit");
 const valid=Boolean(current&&current.principalCurrency&&current.principalOpenBalance>0&&date&&selected&&established&&parsed!==null&&(parsed===undefined||parsed>0)&&note.length<=4096);
 async function refresh(){setBusy(true);setError(null);try{setCurrent(await api.invoiceReminderClaims(slug,id));onChanged();}catch(e){setCurrent(null);setError(e instanceof Error?e.message:"Tabsstatus kunne ikke hentes.");}finally{setBusy(false);}}
 function prepare(){if(!valid||!current||!selected||parsed===null)return;setReview({body:{invoiceDocumentId:id,writeOffDate:date,expenseAccountNo:selected.accountNo,...(parsed!==undefined?{grossAmount:parsed}:{}),...(note.trim()?{note:note.trim()}:{}),confirm:true},currency:current.principalCurrency,invoiceNumber:current.invoiceNumber,openPrincipal:current.principalOpenBalance});}
 return <section aria-label="Konstateret tab"><h4>Konstateret tab</h4>
 <p className="muted">Afskriv kun et konstateret og dokumenteret tab på fakturaens hovedstol. Du bekræfter dokumentationen; serveren afgør ikke selv, om tabet er juridisk konstateret. Gebyr-, kompensations- og rentekrav afskrives ikke i dette trin. Der sendes ingen e-mail.</p>
 {current&&<><p>Aktuel hovedstol: {formatKroner(current.principalOpenBalance,current.principalCurrency)}. Bogførte øvrige restkrav: {formatKroner(current.ledgerBackedClaimBalance,"DKK")}.</p>
 {typeof current.totalBadDebtWrittenOff==="number"&&<p>Afskrevet hovedstol i fakturaens valuta: {formatKroner(current.totalBadDebtWrittenOff,current.principalCurrency)}. Bogført afskrivning i DKK: {formatKroner(current.totalBadDebtWrittenOffDkk,"DKK")}.</p>}
 <ul>{(current.badDebtWriteOffs??[]).map(w=><li key={w.writeOffId}>Tabsafskrivning {w.writeOffId} den {w.writeOffDate} · postering {w.journalEntryNo}. Fakturavaluta: brutto {formatKroner(w.grossAmount,current.principalCurrency)}, netto {formatKroner(w.netAmount,current.principalCurrency)}, moms {formatKroner(w.vatAmount,current.principalCurrency)}. Bogført i DKK: brutto {formatKroner(w.grossAmountDkk,"DKK")}, udgift {formatKroner(w.netAmountDkk,"DKK")}, momsregulering {formatKroner(w.vatAmountDkk,"DKK")}.</li>)}</ul>
 {can("company.ledger.post")&&<div>
 <label>Dato for tabsafskrivning<Input type="date" value={date} onChange={e=>{setDate(e.target.value);setEstablished(false);}}/></label>
 <label>Bruttotab i fakturaens valuta (valgfrit)<Input inputMode="decimal" value={amount} placeholder={`Beløb i ${current.principalCurrency}; tomt = aktuel hovedstol`} onChange={e=>{setAmount(e.target.value);setEstablished(false);}}/></label>
 <p>Beløbet er i {current.principalCurrency}. Lad det stå tomt for at afskrive den aktuelle åbne hovedstol. Serveren genberegner hovedstol, moms og faktisk bogført DKK-værdi på afskrivningsdatoen; der anvendes ingen estimeret kurs her.</p>
 <label>Konto til tab på debitorer<select aria-label="Konto til tab på debitorer" value={account} onChange={e=>{setAccount(e.target.value);setEstablished(false);}}><option value="">Vælg udgiftskonto</option>{current.accounts.filter(a=>a.type==="expense"&&a.normalBalance==="debit").map(a=><option key={a.accountNo} value={a.accountNo}>{a.accountNo} · {a.name}</option>)}</select></label>
 <label>Note til tabsafskrivning<Input value={note} maxLength={4096} onChange={e=>{setNote(e.target.value);setEstablished(false);}}/></label>
 <label><input type="checkbox" checked={established} onChange={e=>setEstablished(e.target.checked)}/>Jeg bekræfter, at tabet er konstateret og dokumenteret.</label>
 <Button type="button" disabled={busy||!valid} onClick={prepare}>Gennemgå tabsafskrivning</Button>
 </div>}</>}
 <Button type="button" variant="secondary" disabled={busy} onClick={()=>void refresh()}>Opdatér tabsstatus</Button>
 {error&&<Banner kind="error">{error}</Banner>}
 {receipt&&<p role="status">Bogført tab i postering {receipt.journalEntryNo} · afskrivning {receipt.writeOffId}. Fakturavaluta: brutto {formatKroner(receipt.grossAmount,receipt.principalCurrency)}, netto {formatKroner(receipt.netAmount,receipt.principalCurrency)}, moms {formatKroner(receipt.vatAmount,receipt.principalCurrency)}. Faktisk bogført i DKK: brutto {formatKroner(receipt.grossAmountDkk,"DKK")}, udgift {formatKroner(receipt.netAmountDkk,"DKK")}, momsregulering {formatKroner(receipt.vatAmountDkk,"DKK")}. Resterende hovedstol {formatKroner(receipt.remainingPrincipal,receipt.principalCurrency)} ({formatKroner(receipt.remainingPrincipalDkk,"DKK")}). Øvrige restkrav {formatKroner(receipt.remainingClaimDkk,"DKK")}.</p>}
 {review&&<ConfirmDialog operationKey={`bad-debt:${slug}:${id}`} title="Bogfør konstateret tab" confirmLabel="Bogfør tab" confirmKind="danger" body={<><p>Faktura {review.invoiceNumber}: afskriv {review.body.grossAmount===undefined?`den aktuelle åbne hovedstol (senest læst ${formatKroner(review.openPrincipal,review.currency)})`:formatKroner(review.body.grossAmount,review.currency)} på {review.body.writeOffDate}. Udgiftskonto {review.body.expenseAccountNo}. Serveren udleder oprindelig debitorkonto, moms og faktisk DKK-værdi. Øvrige krav bevares.</p><p>Du har bekræftet, at tabet er konstateret og dokumenteret.{review.body.note?` Note: ${review.body.note}`:""}</p></>} onConfirm={async()=>{const value=await api.writeOffNativeInvoiceBadDebt(slug,review.body);setReceipt(value);setEstablished(false);await refresh();}} onRefresh={refresh} onClose={()=>setReview(null)}/>}
 </section>;
}
