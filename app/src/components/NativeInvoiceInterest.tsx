import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { api } from "../lib/api";
import type { NativeInvoiceInterestCalculate, NativeInvoiceInterestResult, NativeInvoiceInterestCorrection, NativeInvoiceInterestPost, NativeInvoiceInterestCorrectionPost, NativeReminderClaimStatus } from "../lib/api/invoices";
import { formatKroner, parseDanishAmount } from "../lib/format";
import { useCapabilities } from "../lib/useCapabilities";
import { ConfirmDialog } from "./ConfirmDialog";
import { Banner } from "./Feedback";
import { Button, Input } from "./ui";

type Props={slug:string;id:number;asOfDate:string;principalCurrency:string;accounts:NativeReminderClaimStatus["accounts"];onChanged:()=>void};
type Review={kind:"register";query:NativeInvoiceInterestCalculate}|{kind:"post";query:NativeInvoiceInterestCalculate;body:NativeInvoiceInterestPost}|{kind:"correction";query:NativeInvoiceInterestCalculate|null;body:NativeInvoiceInterestCorrectionPost;proposal:NativeInvoiceInterestCorrection};
export function NativeInvoiceInterest({slug,id,asOfDate,principalCurrency,accounts,onChanged}:Props){
 const {can}=useCapabilities(slug);
 const [date,setDate]=useState(asOfDate),[rate,setRate]=useState(""),[postDate,setPostDate]=useState(asOfDate),[claim,setClaim]=useState(""),[receivable,setReceivable]=useState(""),[income,setIncome]=useState(""),[correctionDate,setCorrectionDate]=useState(asOfDate),[reason,setReason]=useState("");
 const [result,setResult]=useState<NativeInvoiceInterestResult|null>(null),[query,setQuery]=useState<NativeInvoiceInterestCalculate|null>(null),[correction,setCorrection]=useState<NativeInvoiceInterestCorrection|null>(null),[review,setReview]=useState<Review|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState<string|null>(null);
 const supported=principalCurrency==="DKK";
 async function calculate(input?:NativeInvoiceInterestCalculate){
  setBusy(true);setError(null);setResult(null);
  try{const parsed=rate.trim()===""?undefined:parseDanishAmount(rate);if(!input&&(!date||parsed===null))throw new Error("Angiv en dato og en gyldig referencerente, eller lad renten stå tom for serverens regel.");const next=input??{invoiceDocumentId:id,asOfDate:date,...(parsed!==undefined&&parsed!==null?{referenceRatePercent:parsed}:{})};const value=await api.calculateNativeInvoiceInterest(slug,next);setQuery(next);setResult(value);}catch(e){setError(e instanceof Error?e.message:"Renterne kunne ikke hentes.");}finally{setBusy(false);}
 }
 async function readCorrection(clearError=true){setBusy(true);if(clearError)setError(null);setCorrection(null);try{setCorrection(await api.calculateNativeInvoiceInterestCorrection(slug,{invoiceDocumentId:id}));}catch(e){setError(previous=>[previous,e instanceof Error?e.message:"Rentekorrektionen kunne ikke hentes."].filter(Boolean).join(" "));}finally{setBusy(false);}}
 async function refresh(saved:NativeInvoiceInterestCalculate|null){if(saved)await calculate(saved);await readCorrection(false);onChanged();}
 const pending=result?.claims.filter(c=>c.postedJournalEntryId===null)??[],selected=pending.find(c=>c.id===Number(claim));
 const safeCorrection=correction?.canPost===true&&correction.requiresRefund===false&&correction.hasProposal&&correction.overClaimedAmount>0&&correction.proposedLines.length>=2;
 return <section aria-label="Renter"><h4>Renter</h4>
 <p {...stylex.props(cockpitStyles.muted)}>Beregn renter, registrér kravet og bogfør det i særskilte trin. Der sendes ingen e-mail. En tom referencerente bruger serverens daterede regler. Krav og korrektioner er i DKK.</p>
 {!supported&&<Banner kind="warning">Renter understøttes endnu kun for fakturaer i DKK. Fakturaens valuta er {principalCurrency}. Hovedstol og øvrige krav håndteres fortsat i deres respektive forløb.</Banner>}
 <label>Opgørelsesdato for renter<Input type="date" value={date} disabled={busy} onChange={e=>{setDate(e.target.value);setResult(null);setQuery(null);}}/></label>
 <label>Referencerente i procent (valgfri)<Input inputMode="decimal" value={rate} disabled={busy} onChange={e=>{setRate(e.target.value);setResult(null);setQuery(null);}}/></label>
 <Button type="button" variant="secondary" disabled={busy||!supported} onClick={()=>void calculate()}>Beregn renter</Button>
 {error&&<Banner kind="error">{error}</Banner>}
 {result&&<><p>Faktura {result.invoiceNumber} · hovedstol {formatKroner(result.principalOpenBalance,result.principalCurrency)} · forfald {result.effectiveDueDate}. Renter fra {result.interestFromDate} til {result.asOfDate}: {result.claimableDays} dage.</p><p>Referencerente {result.referenceRatePercent}% · årlig rente {result.annualInterestRatePercent}% · {result.referenceRateSource==="manual-override"?"Manuel referencerente":"Serverens daterede referencerente"}. Nyt rentekrav: {formatKroner(result.accruedInterestAmount,"DKK")}.</p>
 {(result.warnings??[]).map(w=><p key={w}>{w}</p>)}{(result.errors??[]).length>0&&<Banner kind="error">{result.errors?.join(" ")}</Banner>}
 <ul>{result.claims.map(c=><li key={c.id}>Rentekrav {c.id} den {c.claimDate}: {formatKroner(c.interestAmount,"DKK")} · {c.postedJournalEntryId===null?"Ikke bogført":`Bogført i ${c.postedJournalEntryNo??c.postedJournalEntryId}`}</li>)}</ul>
 {query&&result.accruedInterestAmount>0&&!(result.errors?.length)&&can("company.draft.write")&&<Button type="button" onClick={()=>setReview({kind:"register",query})}>Gennemgå registrering af renter</Button>}
 {query&&pending.length>0&&can("company.ledger.post")&&<div>
 <label>Registreret rentekrav<select aria-label="Registreret rentekrav" value={claim} onChange={e=>setClaim(e.target.value)}><option value="">Vælg rentekrav</option>{pending.map(c=><option key={c.id} value={c.id}>{c.claimDate} · {formatKroner(c.interestAmount,"DKK")}</option>)}</select></label>
 <label>Bogføringsdato for renter<Input type="date" value={postDate} onChange={e=>setPostDate(e.target.value)}/></label>
 <label>Konto til rentetilgodehavende<select aria-label="Konto til rentetilgodehavende" value={receivable} onChange={e=>setReceivable(e.target.value)}><option value="">Vælg aktivkonto</option>{accounts.filter(a=>a.type==="asset"&&a.normalBalance==="debit").map(a=><option key={a.accountNo} value={a.accountNo}>{a.accountNo} · {a.name}</option>)}</select></label>
 <label>Konto til renteindtægt<select aria-label="Konto til renteindtægt" value={income} onChange={e=>setIncome(e.target.value)}><option value="">Vælg indtægtskonto</option>{accounts.filter(a=>a.type==="income"&&a.normalBalance==="credit").map(a=><option key={a.accountNo} value={a.accountNo}>{a.accountNo} · {a.name}</option>)}</select></label>
 <Button type="button" disabled={!selected||!postDate||!receivable||!income} onClick={()=>{if(selected)setReview({kind:"post",query,body:{invoiceDocumentId:id,claimId:selected.id,transactionDate:postDate,receivableAccountNo:receivable,interestIncomeAccountNo:income,confirm:true}});}}>Gennemgå bogføring af renter</Button>
 </div>}</>}
 <Button type="button" variant="secondary" disabled={busy||!supported} onClick={()=>void readCorrection()}>Beregn rentekorrektion</Button>
 {correction&&<><p>Bogførte renter {formatKroner(correction.postedInterest,"DKK")} · lovlige renter {formatKroner(correction.lawfulInterest,"DKK")} · allerede korrigeret {formatKroner(correction.alreadyCorrected,"DKK")} · nyt korrektionsforslag {formatKroner(correction.overClaimedAmount,"DKK")}.</p>
 {correction.reason&&<p>{correction.reason}</p>}{correction.requiresRefund&&<Banner kind="warning">Kravet kan kræve tilbagebetaling. Automatisk rentekorrektion er ikke tilgængelig.</Banner>}
 <ul>{correction.proposedLines.map((l,i)=><li key={`${l.accountNo}:${i}`}>Konto {l.accountNo}: debet {formatKroner(l.debitAmount,"DKK")}, kredit {formatKroner(l.creditAmount,"DKK")}</li>)}</ul>
 {safeCorrection&&can("company.ledger.post")&&<div><label>Bogføringsdato for rentekorrektion<Input type="date" value={correctionDate} onChange={e=>setCorrectionDate(e.target.value)}/></label><label>Begrundelse for rentekorrektion<Input value={reason} onChange={e=>setReason(e.target.value)}/></label><Button type="button" disabled={!correctionDate||!reason.trim()} onClick={()=>setReview({kind:"correction",query,proposal:correction,body:{invoiceDocumentId:id,transactionDate:correctionDate,reason:reason.trim(),confirm:true}})}>Gennemgå rentekorrektion</Button></div>}</>}
 {review&&<ConfirmDialog key={review.kind} operationKey={`interest:${slug}:${id}:${review.kind}`} title={review.kind==="register"?"Registrér rentekrav":review.kind==="post"?"Bogfør rentekrav":"Bogfør rentekorrektion"} confirmLabel={review.kind==="register"?"Registrér renter":review.kind==="post"?"Bogfør renter":"Bogfør korrektion"} confirmKind="danger" body={<p>{review.kind==="register"?`Registrér rentekrav pr. ${review.query.asOfDate}. Ingen bogføring i dette trin.`:review.kind==="post"?`Bogfør rentekrav ${review.body.claimId} på ${review.body.transactionDate}, debet ${review.body.receivableAccountNo}, kredit ${review.body.interestIncomeAccountNo}. Ingen moms.`:`Bogfør ${review.proposal.overClaimedAmount} DKK i korrektion på ${review.body.transactionDate} med serverens oprindelige konti. Begrundelse: ${review.body.reason}`}</p>} onConfirm={async()=>{if(review.kind==="register")await api.registerNativeInvoiceInterest(slug,{...review.query,confirm:true});else if(review.kind==="post")await api.postNativeInvoiceInterest(slug,review.body);else await api.postNativeInvoiceInterestCorrection(slug,review.body);await refresh(review.query);}} onRefresh={()=>refresh(review.query)} onClose={()=>setReview(null)}/>}
 </section>;
}
