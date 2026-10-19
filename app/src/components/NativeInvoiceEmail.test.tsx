import { afterEach, beforeEach, describe, expect, test, vi } from "bun:test";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NativeInvoiceEmail, knownInvoiceEmailRefusal, validInvoiceEmailNote } from "./NativeInvoiceEmail";
import { api } from "../lib/api";
import { ApiError } from "../lib/api/_shared";
import type { InvoiceEmailRow, InvoiceEmailStatus } from "../lib/api/invoice-email";
import { MutationMemoryProvider } from "../lib/mutation-memory";
import { invoices } from "../test/fixtures";
import { renderAt } from "../test/render";
const row: InvoiceEmailRow = { invoiceDocumentId: 1, invoiceNumber: "2026-00001", kind: "invoice", recipient: "faktura@kunde.dk", subject: "Faktura 2026-00001", messageId: `<${"a".repeat(32)}@rentemester.local>`, bodySha256: "b".repeat(64), pdfSha256: "c".repeat(64), state: "unknown", stateHash: "d".repeat(64), attemptId: "synthetic-attempt", history: [{ type: "reserved" }] };
let status: InvoiceEmailStatus;
beforeEach(() => { sessionStorage.clear(); status = { configured: true, rows: [] }; vi.spyOn(api, "nativeInvoiceEmailStatus").mockImplementation(async () => status); });
afterEach(() => sessionStorage.clear());
function mount(slug = "acme-aps") { return renderAt(<MutationMemoryProvider><NativeInvoiceEmail slug={slug} invoices={invoices().invoices.slice(0,1)} archived={false} onChanged={() => {}} /></MutationMemoryProvider>); }
async function clickPdf(name: string) {
  const link = screen.getByRole("link", { name });
  link.addEventListener("click", e => e.preventDefault(), { once: true });
  await userEvent.click(link);
}
async function prepare() {
  await screen.findByText("Ingen registrerede mailforsøg.");
  await userEvent.selectOptions(screen.getByLabelText("Faktura til mail"), "1");
  expect(screen.getByRole("button", { name: "Gennemgå mailafsendelse" })).toBeDisabled();
  await clickPdf("Åbn fakturaens gemte PDF");
  await userEvent.click(screen.getByLabelText("Jeg har gennemgået modtageren og den gemte PDF"));
  await userEvent.click(screen.getByRole("button", { name: "Gennemgå mailafsendelse" }));
}
async function confirm(title: string, label: string) {
  const dialog = await screen.findByRole("dialog", { name: title });
  await userEvent.click(within(dialog).getByLabelText("Jeg bekræfter den gennemgåede handling"));
  await userEvent.click(within(dialog).getByRole("button", { name: label }));
}
describe("Native invoice mail — reviewed separate effects", () => {
  test("send requires stored PDF/recipient review and one explicit confirmation; reads never send", async () => {
    const send = vi.spyOn(api,"nativeInvoiceEmailSend").mockImplementation(async () => { status={configured:true,rows:[{...row,state:"accepted"}]}; return status.rows[0]!; });
    mount(); await prepare(); expect(send).not.toHaveBeenCalled();
    await confirm("Bekræft mailafsendelse","Send gennemgået mail én gang");
    await waitFor(() => expect(send).toHaveBeenCalledTimes(1));
    expect(send.mock.calls[0]).toEqual(["acme-aps",{invoiceDocumentId:1,to:"faktura@kunde.dk",kind:"invoice",confirm:true}]);
    await screen.findByText(/Kanonisk leveringsstatus genlæses/);
    await userEvent.click(screen.getByRole("button",{name:"Genlæs leveringsstatus"}));
    expect(send).toHaveBeenCalledTimes(1);
  });
  test("definite422 refusal does not leave a browser unknown lock", async () => {
    const send=vi.spyOn(api,"nativeInvoiceEmailSend").mockImplementation(async()=>{status={configured:true,rows:[{...row,state:"refused"}]};throw new ApiError("delivery_refused","Mailen blev afvist af SMTP-serveren.",422);});
    mount();await prepare();await confirm("Bekræft mailafsendelse","Send gennemgået mail én gang");
    await screen.findByRole("button",{name:"Gennemgå nyt forsøg"});
    expect(send).toHaveBeenCalledTimes(1);
    expect(JSON.parse(sessionStorage.getItem("rentemester:uncertain-operations:v1")!)).toEqual([]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  test("network loss preserves payload-free unknown marker across reopen and never resends", async () => {
    const send=vi.spyOn(api,"nativeInvoiceEmailSend").mockImplementation(async()=>{status={configured:true,rows:[row]};throw new ApiError("network","Udfaldet er uafklaret.",0);});
    const view=mount();await prepare();await confirm("Bekræft mailafsendelse","Send gennemgået mail én gang");
    await screen.findByText("Udfaldet er uafklaret.");
    const saved=sessionStorage.getItem("rentemester:uncertain-operations:v1")!;
    expect(JSON.parse(saved)).toHaveLength(1);expect(saved).not.toContain(row.recipient);
    view.unmount();mount();await screen.findByRole("button",{name:"Gennemgå afklaring"});
    expect(send).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("button",{name:"Gennemgå nyt forsøg"})).not.toBeInTheDocument();
  });
  test("unknown reconciliation uses exact statehash and SMTP evidence; refused retry remains a separate PDF-reviewed confirmation",async()=>{
    status={configured:true,rows:[row]};
    const reconcile=vi.spyOn(api,"nativeInvoiceEmailReconcile").mockImplementation(async()=>{status={configured:true,rows:[{...row,state:"refused",stateHash:"e".repeat(64)}]};return status.rows[0]!;});
    const retry=vi.spyOn(api,"nativeInvoiceEmailRetry").mockImplementation(async()=>({...row,state:"accepted"}));
    mount();await userEvent.click(await screen.findByRole("button",{name:"Gennemgå afklaring"}));
    await userEvent.type(screen.getByLabelText("Begrundelse"),"Kontrolleret hos mailadministrator");
    await userEvent.type(screen.getByLabelText("Dokumentation fra SMTP-log"),"SMTP-log reference synthetic-123 viser afvisning");
    await userEvent.click(screen.getByRole("button",{name:"Gennemgå afklaringen"}));
    expect(reconcile).not.toHaveBeenCalled();expect(retry).not.toHaveBeenCalled();
    await confirm("Bekræft afklaring uden afsendelse","Registrér afklaring");
    await waitFor(()=>expect(reconcile).toHaveBeenCalledTimes(1));
    expect(reconcile.mock.calls[0]![1]).toEqual({messageId:row.messageId,expectedStateHash:row.stateHash,reason:"Kontrolleret hos mailadministrator",resolution:"not_accepted",evidence:"SMTP-log reference synthetic-123 viser afvisning",confirm:true});
    await userEvent.click(await screen.findByRole("button",{name:"Gennemgå nyt forsøg"}));
    await userEvent.type(screen.getByLabelText("Begrundelse"),"Nyt kontrolleret enkeltforsøg");
    await clickPdf("Åbn fakturaens gemte PDF før nyt forsøg");
    await userEvent.click(screen.getByLabelText("Jeg har gennemgået den faste modtager og PDF til nyt forsøg"));
    await userEvent.click(screen.getByRole("button",{name:"Gennemgå det nye forsøg"}));
    expect(retry).not.toHaveBeenCalled();await confirm("Bekræft nyt mailforsøg","Send gennemgået mail én gang");
    await waitFor(()=>expect(retry).toHaveBeenCalledTimes(1));
    expect(retry.mock.calls[0]![1].expectedStateHash).toBe("e".repeat(64));
  });
  test("missing SMTP config keeps reader history and owner reconciliation visible without send/retry",async()=>{
    status={configured:false,rows:[row]};mount();
    await screen.findByText(/Mailafsendelse er ikke konfigureret/);
    expect(screen.queryByRole("button",{name:"Gennemgå mailafsendelse"})).not.toBeInTheDocument();
    expect(screen.getByRole("button",{name:"Gennemgå afklaring"})).toBeInTheDocument();
  });
  test("changing company closes frozen intent without sending and archived mode remains read-only",async()=>{
    const send=vi.spyOn(api,"nativeInvoiceEmailSend");
    const view=mount();await prepare();
    view.rerender(<MutationMemoryProvider><NativeInvoiceEmail slug="other-synthetic" invoices={invoices().invoices.slice(0,1)} archived={true} onChanged={()=>{}} /></MutationMemoryProvider>);
    await screen.findByText("Ingen registrerede mailforsøg.");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("button",{name:"Gennemgå mailafsendelse"})).not.toBeInTheDocument();
    expect(send).not.toHaveBeenCalled();
  });
  test("legacy accepted rows explicitly disclose missing PDF proof without offering resend",async()=>{
    status={configured:true,rows:[{...row,state:"accepted",pdfSha256:null,legacy:true}]};mount();
    await userEvent.click(await screen.findByText("Vis historik og dokumentation"));
    expect(screen.getByText(/Historisk accepteret afsendelse uden bevaret PDF-hash/)).toBeInTheDocument();
    expect(screen.queryByRole("button",{name:"Gennemgå nyt forsøg"})).not.toBeInTheDocument();
  });
  test("owner attestation and historical logs are not presented as a fresh SMTP receipt", async () => {
    status={configured:true,rows:[{...row,state:"accepted",humanResolution:true}]};
    const view=mount();await screen.findByText(/Accepteret efter ejerens afklaring/);
    expect(screen.queryByText(/SMTP-serveren har accepteret mailen/)).not.toBeInTheDocument();
    view.unmount();status={configured:true,rows:[{...row,state:"accepted",legacy:true,pdfSha256:null}]};
    mount();await screen.findByText(/Historisk registreret afsendelse/);
    expect(screen.queryByText(/SMTP-serveren har accepteret mailen/)).not.toBeInTheDocument();
  });
  test("unknown classification and note syntax preserve bounds without treating ambiguous errors as refusal",()=>{
    for(const status of [0,200,408,500,503])expect(knownInvoiceEmailRefusal(new ApiError("internal","x",status))).toBe(false);
    expect(knownInvoiceEmailRefusal(new ApiError("delivery_unknown","x",409))).toBe(false);
    expect(knownInvoiceEmailRefusal(new ApiError("delivery_refused","x",422))).toBe(true);
    expect(validInvoiceEmailNote("Concrete source reason",2000)).toBe(true);
    expect(validInvoiceEmailNote("line\nbreak reason",2000)).toBe(false);
    expect(validInvoiceEmailNote("æ".repeat(1001),2000)).toBe(false);
  });
});
