import { type Page } from "@playwright/test";

/** Real Rust commit, deliberately lost browser reply, then read-back and exact replay. */
export async function checkNativeLostPosting(page: Page, base: string) {
  const slug="synthetic",path=`/api/companies/${slug}/manual-journal`;
  const text="Synthetic accepted posting with lost browser reply";
  const need=(value:unknown,message:string)=>{if(!value)throw new Error(message);};
  const journal=async()=>{const r=await page.request.get(`${base}/api/companies/${slug}/journal`);need(r.ok(),"Canonical journal readable");return (await r.json()).journal;};
  const before=await journal();let writes=0;let original:unknown;
  let accepted:{entryId:number;entryNo:string;entryHash:string;receipt:{replayed:boolean;receiptId:number}}|undefined;
  await page.route(`**${path}`,async route=>{
    if(route.request().method()!=="POST")return route.continue();
    writes++;original=route.request().postDataJSON();const response=await route.fetch();need(response.ok(),"Lost write actually committed");accepted=await response.json();need(accepted?.receipt?.replayed===false,"Fresh intent has one durable receipt");await route.abort("connectionreset");
  });
  async function fillForm(value:string,amount:string) {
    const form=page.getByRole("region",{name:"Ny manuel postering",exact:true});
    await form.getByLabel("Posteringstekst",{exact:true}).fill(value);
    await form.getByLabel("Debetkonto",{exact:true}).selectOption("1759");
    await form.getByLabel("Kreditkonto",{exact:true}).selectOption("8759");
    await form.getByLabel("Beløb i DKK",{exact:true}).fill(amount);
    await form.getByRole("button",{name:"Gennemse postering",exact:true}).click();
    return page.getByRole("dialog",{name:"Bogfør postering",exact:true});
  }
  try {
    await page.goto(`${base}/companies/${slug}/posteringer`);
    await page.getByRole("button",{name:"Ny postering",exact:true}).click();
    let dialog=await fillForm(text,"9.87");
    await dialog.getByRole("button",{name:"Bogfør postering",exact:true}).click();
    await dialog.getByRole("button",{name:"Resultatet er afklaret",exact:true}).waitFor();
    need(await dialog.getByRole("button",{name:"Bogfør postering",exact:true}).isDisabled(),"Unknown reply blocks second write");
    await dialog.getByRole("button",{name:"Annullér",exact:true}).click();
    await page.getByRole("region",{name:"Ny manuel postering",exact:true}).getByRole("button",{name:"Annullér",exact:true}).click();
    await page.goto(`${base}/companies/${slug}/posteringer/`);
    await page.getByRole("button",{name:"Ny postering",exact:true}).click();
    const recovery=page.getByRole("region",{name:"Tidligere bogføringskvitteringer",exact:true});
    await recovery.getByText(`Kvittering ${accepted!.receipt.receiptId}`,{exact:false}).waitFor();
    need(await page.getByRole("region",{name:"Ny manuel postering",exact:true}).count()===0,"Full reload/equivalent route preserves recovery barrier");
    need(await recovery.getByRole("button",{name:"Bekræft kvittering",exact:true}).isDisabled(),"Acknowledgement requires explicit review");
    await recovery.getByRole("button",{name:"Kontrollér status",exact:true}).click();
    await recovery.getByRole("checkbox").waitFor();
    need(await page.getByRole("region",{name:"Ny manuel postering",exact:true}).count()===0,"Read alone cannot release receipt barrier");
    const once=await journal();
    need(once.entries.length===before.entries.length+1 && once.entries.filter((e:{text:string})=>e.text===text).length===1,"Exactly one canonical posting");
    need(writes===1 && accepted && original,"Only original financial intent sent");
    const replay=await page.request.post(`${base}${path}`,{headers:{origin:base},data:original});need(replay.ok(),"Original envelope can recover authoritative receipt");
    const receipt=await replay.json();
    need(receipt.receipt?.replayed===true && receipt.entryId===accepted!.entryId && receipt.entryNo===accepted!.entryNo && receipt.entryHash===accepted!.entryHash,"Replay returns same original result");
    need(JSON.stringify(await journal())===JSON.stringify(once),"Replay changes no ledger");
    await recovery.getByRole("checkbox").check();await recovery.getByRole("button",{name:"Bekræft kvittering",exact:true}).click();
    dialog=await fillForm("Synthetic new intent after explicit recovery","1.00");
    need(await dialog.getByRole("button",{name:"Bogfør postering",exact:true}).isEnabled(),"Explicit server receipt review also releases stale tab barrier");
    need(writes===1 && JSON.stringify(await journal())===JSON.stringify(once),"Acknowledgement and release send no financial action");
    await dialog.getByRole("button",{name:"Annullér",exact:true}).click();
    await page.getByRole("region",{name:"Ny manuel postering",exact:true}).getByRole("button",{name:"Annullér",exact:true}).click();
    await page.getByText(text,{exact:true}).waitFor();
    return {acceptedReplyDiscarded:true,browserWrites:writes,blockSurvivesReloadAndRouteVariant:true,exactReceiptReplay:true,explicitReleaseWithoutWrite:true,entryId:receipt.entryId,entryHash:receipt.entryHash};
  } finally {await page.unroute(`**${path}`);}
}
