import type { Page } from "@playwright/test";
import { readFileSync,writeFileSync } from "node:fs";
const path="/tmp/rentemester-rust-reports/native-vendor.json";
export async function createNativeVendor(page:Page,base:string) {
  await page.goto(`${base}/companies/synthetic/kontakter`);
  await page.getByRole("button",{name:"Tilføj leverandør",exact:true}).first().click();
  const dialog=page.getByRole("dialog");
  for(const [label,value] of Object.entries({Navn:"Synthetic Supplier",Adresse:"Synthetic Supplier Street 2","Leverandørland (ISO)":"DK","Standard udgiftskonto":"3100",Telefon:"11111111","E-mail":"supplier@example.invalid",Noter:"Synthetic vendor original"})) await dialog.getByLabel(label,{exact:true}).fill(value);
  await dialog.getByLabel(/^CVR \/ moms-nr\./).fill("12345678");
  await dialog.getByLabel(/^Leverandøridentitet/).selectOption("dk_cvr");
  await dialog.getByLabel(/^Momsbehandling/).selectOption("standard");
  const [response]=await Promise.all([page.waitForResponse(r=>r.url().endsWith("/vendors")&&r.request().method()==="POST"),dialog.getByRole("button",{name:"Opret",exact:true}).click()]);
  const result=await response.json();
  if(!response.ok()) throw new Error(`Vendor create failed: ${JSON.stringify(result)}`);
  await dialog.waitFor({state:"hidden"});
  writeFileSync(path,JSON.stringify({id:result.vendor.id}));
  return result.vendor.id as number;
}
export async function checkNativeVendor(page:Page,base:string,reopened=false) {
  const url=`${base}/api/companies/synthetic`;
  const headers={origin:base,"content-type":"application/json"};
  const evidence: {id:number;documentId?:number;original?:string}=JSON.parse(readFileSync(path,"utf8"));
  if(!reopened) {
    const docs=await(await page.request.get(`${url}/documents`)).json();
    const document=docs.documents.documents.find((d:{filename:string})=>d.filename==="synthetic-domestic-vat.txt");
    if(!document||!document.journalEntryId) throw new Error("Vendor purchase was not booked");
    evidence.documentId=document.id;
    const original=await page.request.get(`${url}/documents/${document.id}/file`);
    if(!original.ok()) throw new Error("Original purchase evidence missing");
    evidence.original=(await original.body()).toString("base64");
    await page.goto(`${base}/companies/synthetic/kontakter`);
    await page.getByRole("button",{name:"Redigér Synthetic Supplier",exact:true}).click();
    const dialog=page.getByRole("dialog");
    await dialog.getByLabel("Navn",{exact:true}).fill("Synthetic Updated Supplier");
    await dialog.getByLabel("Adresse",{exact:true}).fill("Synthetic New Supplier Street 3");
    await dialog.getByLabel("Telefon",{exact:true}).fill("");
    await dialog.getByLabel(/^Noter/).fill("");
    const [response]=await Promise.all([page.waitForResponse(r=>r.url().endsWith(`/vendors/${evidence.id}`)&&r.request().method()==="PATCH"),dialog.getByRole("button",{name:"Gem ændringer",exact:true}).click()]);
    if(!response.ok()) throw new Error(`Vendor edit failed: ${await response.text()}`);
    await dialog.waitFor({state:"hidden"});
    for(const data of [{countryCode:"DE"},{defaultExpenseAccount:"1800"},{name:" "},{unknownField:true}]) {
      const invalid=await page.request.patch(`${url}/vendors/${evidence.id}`,{headers,data});
      if(invalid.status()!==422) throw new Error("Invalid vendor identity/account accepted");
    }
    writeFileSync(path,JSON.stringify(evidence));
  }
  const contacts=await(await page.request.get(`${url}/contacts`)).json();
  const vendor=contacts.contacts.vendors.find((v:{id:number})=>v.id===evidence.id);
  if(!vendor||vendor.name!=="Synthetic Updated Supplier"||vendor.address!=="Synthetic New Supplier Street 3"||vendor.phone!==null||vendor.notes!==null||vendor.email!=="supplier@example.invalid"||vendor.vatOrCvr!=="DK12345678"||vendor.countryCode!=="DK"||vendor.identifierKind!=="dk_cvr"||vendor.identityStatus!=="resolved"||vendor.defaultExpenseAccount!=="3100"||vendor.defaultVatTreatment!=="standard") throw new Error(`Vendor master data wrong: ${JSON.stringify(vendor)}`);
  const docs=await(await page.request.get(`${url}/documents`)).json();
  const document=docs.documents.documents.find((d:{id:number})=>d.id===evidence.documentId);
  if(document.supplierName!=="Synthetic Supplier"||document.supplierVatOrCvr!=="DK12345678"||document.supplierCountryCode!=="DK"||document.supplierIdentifierKind!=="dk_cvr"||!document.journalEntryId) throw new Error("Vendor edit rewrote historical purchase identity");
  const original=await page.request.get(`${url}/documents/${evidence.documentId}/file`);
  if(!(await original.body()).equals(Buffer.from(evidence.original!,"base64"))) throw new Error("Vendor edit changed original purchase bytes");
  await page.goto(`${base}/companies/synthetic/kontakter`);
  await page.getByRole("button",{name:"Redigér Synthetic Updated Supplier",exact:true}).click();
  await page.getByRole("dialog").getByLabel("Adresse",{exact:true}).waitFor();
  await page.screenshot({path:"/tmp/rentemester-rust-vendor.png",fullPage:true});
  await page.getByRole("dialog").getByRole("button",{name:"Annullér",exact:true}).click();
  console.log(reopened?"PASS: typed vendor identity and edits persist after restart; booked purchase retains original supplier snapshot and exact file":"PASS: browser creates typed vendor → selects vendor on purchase → books/reconciles purchase → edits vendor without changing original evidence; invalid identity/default account refused");
}
