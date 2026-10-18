import type { Page } from "@playwright/test";
import {readFileSync,writeFileSync} from "node:fs";
const saved="/tmp/rentemester-rust-reports/native-assets.json";
export async function checkNativeAssets(page:Page,base:string,restarted=false){
 const url=`${base}/api/companies/synthetic`,headers={origin:base,"content-type":"application/json"};
 let evidence:{assetId:number;documentId:number;baselineExpense:number;baselineResult:number};
 if(!restarted){
  const baseline=await(await page.request.get(`${url}/income-statement`)).json();
  await page.goto(`${base}/companies/synthetic/kontoplan`);
  for(const [no,name,type,balance] of [["1500","Synthetic equipment","asset","debit"],["1590","Synthetic accumulated depreciation","asset","credit"],["3900","Synthetic depreciation expense","expense","debit"]]){
   const form=page.getByRole("form",{name:"Opret konto",exact:true});await form.getByLabel("Kontonummer",{exact:true}).fill(no);await form.getByLabel("Kontonavn",{exact:true}).fill(name);await form.getByLabel(/^Kontotype/).selectOption(type);await form.getByLabel(/^Normal saldo/).selectOption(balance);await form.getByRole("button",{name:"Opret konto",exact:true}).click();await page.getByRole("row").filter({hasText:name}).waitFor();
  }
  await page.goto(`${base}/companies/synthetic/bilag`);await page.getByRole("button",{name:"Indlæs bilag",exact:true}).click();const upload=page.getByRole("dialog");
  const today=new Date().toLocaleDateString("sv-SE");
  await upload.getByLabel("Bilagsfil",{exact:true}).setInputFiles({name:"synthetic-equipment.txt",mimeType:"text/plain",buffer:Buffer.from("Synthetic equipment acquired through owner contribution; cost DKK1000, no deductible VAT\n")});
  await upload.getByLabel("Afsender",{exact:true}).fill("Synthetic Equipment Supplier");
  await upload.getByLabel("Afsender CVR/moms",{exact:true}).fill("DK12345678");
  await upload.getByLabel("Afsenderadresse",{exact:true}).fill("Synthetic Supplier 2");
  await upload.getByLabel("Leverandørland (ISO)",{exact:true}).fill("DK");
  await upload.getByLabel(/^Identitetstype/).selectOption("dk_cvr");
  for(const[label,value]of Object.entries({"Bilagsdato":today,"Fakturanr. (valgfri)":"SYN-ASSET-1","Beløb inkl. moms":"1000,00","Momsbeløb":"0,00","Beskrivelse af leverance":"Synthetic equipment","Modtager":"Synthetic Company","Modtager CVR/moms":"DK87654321","Modtageradresse":"Synthetic Street 1"}))await upload.getByLabel(label,{exact:true}).fill(value);
  await upload.getByRole("button",{name:"Indlæs bilag",exact:true}).click();await upload.getByText(/Bilaget er indlæst som DOC-/).waitFor();await upload.getByRole("button",{name:"Luk",exact:true}).click();
  const docs=await(await page.request.get(`${url}/documents`)).json();const document=docs.documents.documents.find((d:{filename:string})=>d.filename==="synthetic-equipment.txt");if(!document)throw new Error("Asset purchase original missing");
  const wire={name:"Synthetic equipment",category:"equipment",acquisitionDate:today,cost:1000,usefulLifeMonths:3,purchaseDocumentId:document.id,assetAccountNo:"1500",depreciationExpenseAccountNo:"3900",accumulatedDepreciationAccountNo:"1590",confirm:true};
  const unbooked=await page.request.post(`${url}/assets`,{headers,data:wire});if(unbooked.status()!==422)throw new Error("Uncapitalised purchase registered as booked asset");
  await page.goto(`${base}/companies/synthetic/posteringer`);await page.getByRole("button",{name:"Ny postering",exact:true}).click();
  await page.getByLabel("Posteringstekst",{exact:true}).fill("Synthetic equipment owner contribution");await page.getByLabel(/^Bilag \(valgfrit\)/).selectOption(String(document.id));await page.getByLabel("Debetkonto",{exact:true}).selectOption("1500");await page.getByLabel("Kreditkonto",{exact:true}).selectOption("8000");await page.getByLabel("Beløb i DKK",{exact:true}).fill("1000,00");await page.getByRole("button",{name:"Gennemse postering",exact:true}).click();await page.getByRole("dialog").getByRole("button",{name:"Bogfør postering",exact:true}).click();await page.getByText("Synthetic equipment owner contribution",{exact:true}).waitFor();
  await page.goto(`${base}/companies/synthetic/anlaeg`);await page.getByRole("button",{name:"Registrér anlæg",exact:true}).click();const dialog=page.getByRole("dialog");
  await dialog.getByLabel("Navn",{exact:true}).fill("Synthetic equipment");await dialog.getByLabel("Kategori",{exact:true}).fill("equipment");await dialog.getByLabel("Kostpris (kr.)",{exact:true}).fill("1000,00");await dialog.getByLabel("Levetid (måneder, lineær afskrivning)",{exact:true}).fill("3");await dialog.getByLabel(/^Bilag \(købsdokument\)/).selectOption(String(document.id));await dialog.getByLabel(/^Anlægskonto/).selectOption("1500");await dialog.getByLabel(/^Afskrivningskonto/).selectOption("3900");await dialog.getByLabel(/^Akkumulerede afskrivninger/).selectOption("1590");
  const [registered]=await Promise.all([page.waitForResponse(r=>r.url().endsWith("/assets")&&r.request().method()==="POST"),dialog.getByRole("button",{name:"Registrér anlæg",exact:true}).click()]);if(!registered.ok())throw new Error(`Asset registration failed: ${await registered.text()}`);const asset=(await registered.json()).asset;
  await dialog.waitFor({state:"hidden"});evidence={assetId:asset.assetId,documentId:document.id,baselineExpense:baseline.incomeStatement.totalExpense,baselineResult:baseline.incomeStatement.result};
  const duplicate=await page.request.post(`${url}/assets`,{headers,data:wire});if(duplicate.status()!==422)throw new Error("Already allocated acquisition registered again");
  const row=page.getByRole("row").filter({hasText:"Synthetic equipment"});
  await row.getByRole("button",{name:"Beregn afskrivning for Synthetic equipment",exact:true}).click();const confirmation=page.getByRole("dialog");await confirmation.getByText(/333,33/).waitFor();
  const [posted]=await Promise.all([page.waitForResponse(r=>r.url().endsWith("/depreciate")&&r.request().method()==="POST"),confirmation.getByRole("button",{name:"Bogfør afskrivning",exact:true}).click()]);if(!posted.ok())throw new Error(`Depreciation failed: ${await posted.text()}`);if((await posted.json()).depreciation.periodAmount!==333.33)throw new Error("First depreciation rounded incorrectly");await confirmation.waitFor({state:"hidden"});await row.getByText("1/3 afskrevet",{exact:true}).waitFor();
  const replay=await page.request.post(`${url}/assets/${evidence.assetId}/depreciate`,{headers,data:{confirm:true,periodIndex:1,transactionDate:today}});if(replay.status()!==422)throw new Error("Depreciation period posted twice");
  writeFileSync(saved,JSON.stringify(evidence));
 }else evidence=JSON.parse(readFileSync(saved,"utf8"));
 const listed=await(await page.request.get(`${url}/assets`)).json();const asset=listed.assets.assets.find((a:{assetId:number})=>a.assetId===evidence.assetId);
 if(!asset||asset.cost!==1000||asset.accumulatedDepreciation!==333.33||asset.netBookValue!==666.67||asset.postedPeriods!==1)throw new Error(`Asset register wrong: ${JSON.stringify(listed)}`);
 const next=await(await page.request.get(`${url}/assets/${evidence.assetId}/next-depreciation`)).json();if(next.nextDepreciation.nextPeriodIndex!==2||next.nextDepreciation.nextPeriodAmount!==333.33)throw new Error("Next depreciation preview wrong");
 const report=await(await page.request.get(`${url}/income-statement`)).json();if(Math.abs(report.incomeStatement.totalExpense-evidence.baselineExpense-333.33)>0.00001||Math.abs(report.incomeStatement.result-evidence.baselineResult+333.33)>0.00001)throw new Error("Depreciation did not reach financial reports");
 const trial=await(await page.request.get(`${url}/trial-balance`)).json();if(!trial.trialBalance.balanced)throw new Error("Depreciation unbalanced ledger");
 const journals=await(await page.request.get(`${url}/journal`)).json();const acquisition=journals.journal.entries.find((j:{text:string})=>j.text==="Synthetic equipment owner contribution");if(acquisition.documentId!==evidence.documentId)throw new Error("Acquisition lost purchase link");
 const original=await page.request.get(`${url}/documents/${evidence.documentId}/file`);if(!original.ok()||!(await original.text()).includes("Synthetic equipment acquired"))throw new Error("Asset original unavailable");
 await page.goto(`${base}/companies/synthetic/resultatopgorelse`);await page.getByText("Synthetic depreciation expense",{exact:true}).waitFor();await page.goto(`${base}/companies/synthetic/anlaeg`);await page.getByRole("row").filter({hasText:"Synthetic equipment"}).getByText("1/3 afskrevet",{exact:true}).waitFor();await page.screenshot({path:"/tmp/rentemester-rust-assets.png",fullPage:true});
 console.log(restarted?"PASS: asset, purchase original, depreciation, next period preview and financial reports persist after restart":"PASS: purchase original → manual capitalisation with document → explicit asset accounts → registration → preview 333.33 → depreciation → 666.67 book value and correct financial reports; unbooked, duplicate allocation and duplicate period refused");
}
