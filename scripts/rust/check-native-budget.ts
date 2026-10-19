import type { Page } from "@playwright/test";
import {mkdirSync, readFileSync, writeFileSync} from "node:fs";
const directory="/tmp/rentemester-rust-reports";
const saved=`${directory}/native-budget.json`;
type Evidence={period:string;year:string;firstId:number;lastId:number;entryId:number};
/** Existing authenticated synthetic browser session; safe beside other focused journeys. */
export async function checkNativeBudget(page:Page,base:string,restarted=false){
 const url=`${base}/api/companies/synthetic`,headers={origin:base,"content-type":"application/json"};
 const no="3710",name="Synthetic budget expense";
 let evidence:Evidence;
 const read=async(path:string)=>{const response=await page.request.get(`${url}/${path}`);if(!response.ok())throw new Error(`${path}: ${await response.text()}`);return response.json();};
 if(!restarted){
  mkdirSync(directory,{recursive:true});
  await page.goto(`${base}/companies/synthetic/kontoplan`);
  const form=page.getByRole("form",{name:"Opret konto",exact:true});
  await form.getByLabel("Kontonummer",{exact:true}).fill(no);await form.getByLabel("Kontonavn",{exact:true}).fill(name);await form.getByLabel(/^Kontotype/).selectOption("expense");await form.getByLabel(/^Normal saldo/).selectOption("debit");await form.getByRole("button",{name:"Opret konto",exact:true}).click();await page.getByRole("row").filter({hasText:name}).waitFor();
  await page.goto(`${base}/companies/synthetic/bilag`);await page.getByRole("button",{name:"Indlæs bilag",exact:true}).click();const upload=page.getByRole("dialog");
  const today=new Date().toLocaleDateString("sv-SE");
  await upload.getByLabel("Bilagsfil",{exact:true}).setInputFiles({name:"synthetic-budget-expense.txt",mimeType:"text/plain",buffer:Buffer.from("Synthetic budget expense DKK12.34 without VAT, paid by owner.\n")});
  await upload.getByLabel("Afsender",{exact:true}).fill("Synthetic Budget Supplier");await upload.getByLabel("Afsender CVR/moms",{exact:true}).fill("DK12345678");await upload.getByLabel("Afsenderadresse",{exact:true}).fill("Synthetic Supplier 2");await upload.getByLabel("Leverandørland (ISO)",{exact:true}).fill("DK");await upload.getByLabel(/^Identitetstype/).selectOption("dk_cvr");
  for(const[label,value]of Object.entries({"Bilagsdato":today,"Fakturanr. (valgfri)":"SYN-BUDGET-1","Beløb inkl. moms":"12,34","Momsbeløb":"0,00","Beskrivelse af leverance":"Synthetic budget expense","Modtager":"Synthetic Company","Modtager CVR/moms":"DK87654321","Modtageradresse":"Synthetic Street 1"}))await upload.getByLabel(label,{exact:true}).fill(value);
  await upload.getByRole("button",{name:"Indlæs bilag",exact:true}).click();await upload.getByText(/Bilaget er indlæst som DOC-/).waitFor();await upload.getByRole("button",{name:"Luk",exact:true}).click();
  const documents=(await read("documents")).documents.documents;const document=documents.find((d:{filename:string})=>d.filename==="synthetic-budget-expense.txt");if(!document)throw new Error("Synthetic budget original missing");
  await page.goto(`${base}/companies/synthetic/posteringer`);await page.getByRole("button",{name:"Ny postering",exact:true}).click();
  await page.getByLabel("Posteringstekst",{exact:true}).fill("Synthetic budget known actual");await page.getByLabel(/^Bilag \(valgfrit\)/).selectOption(String(document.id));await page.getByLabel("Debetkonto",{exact:true}).selectOption(no);await page.getByLabel("Kreditkonto",{exact:true}).selectOption("8000");await page.getByLabel("Beløb i DKK",{exact:true}).fill("12,34");await page.getByRole("button",{name:"Gennemse postering",exact:true}).click();await page.getByRole("dialog").getByRole("button",{name:"Bogfør postering",exact:true}).click();await page.getByText("Synthetic budget known actual",{exact:true}).waitFor();
  const journal=(await read("journal")).journal;
  const entry=journal.entries.find((e:{text:string})=>e.text==="Synthetic budget known actual");if(!entry)throw new Error("Known budget actual missing");
  const period=entry.date.slice(0,7);
  await page.goto(`${base}/companies/synthetic/budget`);await page.getByRole("heading",{name:"Budget",exact:true}).waitFor();
  await page.getByLabel("Kontonr.",{exact:true}).fill(no);await page.getByLabel("Måned",{exact:true}).selectOption(period);await page.getByLabel("Beløb",{exact:true}).fill("20,00");
  const [created]=await Promise.all([page.waitForResponse(r=>r.url().endsWith("/budget")&&r.request().method()==="POST"),page.getByRole("button",{name:"Tilføj budgetlinje",exact:true}).click()]);if(!created.ok())throw new Error(`Budget creation failed: ${await created.text()}`);const firstId=(await created.json()).budget.id;
  const cell=page.getByLabel(`Budget for konto ${no} ${period}`,{exact:true});await cell.waitFor();await cell.fill("30,01");
  const [updated]=await Promise.all([page.waitForResponse(r=>r.url().endsWith("/budget")&&r.request().method()==="POST"),cell.press("Enter")]);if(!updated.ok())throw new Error(`Budget update failed: ${await updated.text()}`);const lastId=(await updated.json()).budget.id;
  if(lastId<=firstId)throw new Error("Budget edit did not append a revision");
  const after=(await read("journal")).journal;if(after.entries.length!==journal.entries.length)throw new Error("Budget planning mutated journal");
  const invalid=await page.request.post(`${url}/budget`,{headers,data:{accountNo:no,period:"2026-13",amount:1}});if(invalid.status()!==422)throw new Error("Invalid calendar budget accepted");
  const spoof=await page.request.post(`${url}/budget`,{headers,data:{accountNo:no,period,amount:1,actor:"user:other"}});if(spoof.status()!==400)throw new Error("Caller supplied budget actor accepted");
  evidence={period,year:journal.selectedYear,firstId,lastId,entryId:entry.id};writeFileSync(saved,JSON.stringify(evidence,null,2));
 }else evidence=JSON.parse(readFileSync(saved,"utf8"));
 const query=`?year=${encodeURIComponent(evidence.year)}`;
 const plan=(await read(`budget${query}`)).budget;
 const line=plan.lines.find((l:{accountNo:string;period:string})=>l.accountNo===no&&l.period===evidence.period);
 if(!line||line.id!==evidence.lastId||line.amount!==30.01||plan.dimensionActualsAvailable!==false)throw new Error(`Effective budget wrong: ${JSON.stringify(plan)}`);
 const comparison=(await read(`budget-vs-actual${query}`)).budgetVsActual;
 const actual=comparison.lines.find((l:{accountNo:string;period:string})=>l.accountNo===no&&l.period===evidence.period);
 if(!actual||actual.budget!==30.01||actual.actual!==12.34||actual.variance!==17.67)throw new Error(`Budget comparison wrong: ${JSON.stringify(actual)}`);
 await page.goto(`${base}/companies/synthetic/budget${query}`);const cell=page.getByLabel(`Budget for konto ${no} ${evidence.period}`,{exact:true});await cell.waitFor();if(await cell.inputValue()!=="30.01")throw new Error("Reloaded cell lost budget revision");
 await page.getByRole("button",{name:"Sammenlign med faktisk",exact:true}).click();const row=page.getByRole("row").filter({hasText:name});await row.waitFor();for(const text of ["30,01","12,34","17,67"]){if(!(await row.innerText()).includes(text))throw new Error(`UI comparison missing ${text}`);}
 await page.getByText(/Dimensionssammenligning er endnu ikke tilgængelig/).waitFor();
 await page.reload();await page.getByLabel(`Budget for konto ${no} ${evidence.period}`,{exact:true}).waitFor();
 await page.screenshot({path:`${directory}/native-budget${restarted?"-restarted":""}.png`,fullPage:true});
 console.log(restarted?"PASS: account budget revision, known actual and variance persist across native server restart":"PASS: React budget create → append revision → exact known actual and variance → reload; no journal mutation, invalid month and spoofed actor refused; unavailable dimensions explicit");
}
