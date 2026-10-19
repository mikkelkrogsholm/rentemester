import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockFetch } from "../test/fixtures";
import { restoreGlobals } from "../test/globals";
import { renderAt } from "../test/render";
const user={id:"synthetic-owner",email:"owner@example.invalid",emailVerified:true,twoFactorEnabled:true};
mock.module("../lib/auth-client",()=>({authClient:{getSession:async()=>({data:{user,session:{id:"synthetic-session"}},error:null})}}));
const {AuthProvider}=await import("../lib/auth-context");
const {MutationMemoryProvider}=await import("../lib/mutation-memory");
const {WorkspaceTransferProvider,WorkspaceReadOnlyGuard,useWorkspaceTransfer}=await import("../components/WorkspaceTransferProvider");
const {WorkspaceHandoffView,transferNote}=await import("./WorkspaceHandoffView");
const {clearWorkspaceWriteMode}=await import("../lib/workspace-write-mode");
const active={mode:"active",stateHash:"a".repeat(64),destination:null,transferId:null,challenge:null,release:null,offeredArchiveHashes:[],pendingEffects:[],history:[]};
const frozen={...active,mode:"frozen",stateHash:"b".repeat(64),destination:"Synthetic destination"};
const me={user,workspaceRole:"workspace_owner",companies:[{slug:"a",name:"Synthetic",role:"owner",archived:false}]};
function mount(){return renderAt(<AuthProvider hosted backend="rust"><MutationMemoryProvider><WorkspaceTransferProvider><WorkspaceHandoffView/></WorkspaceTransferProvider></MutationMemoryProvider></AuthProvider>,{route:"/flytning"});}
const posts=()=>(fetch as any).mock.calls.filter(([,i]:any[])=>i?.method==="POST");
beforeEach(()=>sessionStorage.clear());
afterEach(()=>{sessionStorage.clear();clearWorkspaceWriteMode(`${window.location.origin}:synthetic-owner:synthetic-session`);restoreGlobals();});
async function freeze(){await waitFor(()=>expect(screen.getByRole("button",{name:"Gennemgå frysning"})).not.toBeDisabled());await userEvent.type(screen.getByLabelText("Destination"),"Synthetic destination");await userEvent.type(screen.getByLabelText("Begrundelse"),"Owner reviewed destination");await userEvent.click(screen.getByRole("button",{name:"Gennemgå frysning"}));const d=await screen.findByRole("dialog",{name:"Bekræft frysning"});expect(posts()).toHaveLength(0);await userEvent.click(within(d).getByRole("checkbox",{name:"Jeg bekræfter den gennemgåede handling"}));await userEvent.click(within(d).getByRole("button",{name:"Frys workspace"}));}
test("freeze sends exact frozen intent only after separate explicit confirmation",async()=>{
 mockFetch({"GET /api/me":me,"GET /api/workspace/transfer":{transfer:active},"POST /api/workspace/transfer/freeze":{transfer:frozen}});mount();await freeze();await waitFor(()=>expect(posts()).toHaveLength(1));expect(JSON.parse(posts()[0][1].body)).toEqual({expectedStateHash:active.stateHash,destination:"Synthetic destination",reason:"Owner reviewed destination",confirm:true});
});
test("moved source retains release but never claims target activation",async()=>{
 mockFetch({"GET /api/me":me,"GET /api/workspace/transfer":{transfer:{...frozen,mode:"moved",release:"signed-envelope",offeredArchiveHashes:["c".repeat(64)]}}});mount();await screen.findByRole("heading",{name:"Kilden er permanent skrivebeskyttet"});expect(screen.getByLabelText("Signeret frigivelse")).toHaveValue("signed-envelope");expect(screen.queryByRole("button",{name:"Gennemgå aktivering"})).toBeNull();expect(screen.getByText(/Den beviser ikke, at målet er blevet aktiveret/)).toBeInTheDocument();expect(posts()).toHaveLength(0);
});
test("malformed mutation receipt keeps payload-free unknown marker without retry",async()=>{
 mockFetch({"GET /api/me":me,"GET /api/workspace/transfer":{transfer:active},"POST /api/workspace/transfer/freeze":{nonsense:true}});mount();await freeze();await screen.findByText(/Workspace-tilstanden kunne ikke bekræftes/);expect(screen.getByRole("button",{name:"Frys workspace"})).toBeDisabled();const saved=sessionStorage.getItem("rentemester:uncertain-operations:v1")!;expect(JSON.parse(saved)).toHaveLength(1);expect(saved).not.toContain("Synthetic");expect(posts()).toHaveLength(1);
});
function Probe(){const lifecycle=useWorkspaceTransfer()!;return <><button onClick={()=>lifecycle.adopt(frozen as any)}>Freeze mode</button><WorkspaceReadOnlyGuard><button>Existing dialog confirm</button><a href="/reports">Read report</a></WorkspaceReadOnlyGuard></>;}
test("mode transition disables already-open controls and retains report links",async()=>{
 mockFetch({"GET /api/me":me,"GET /api/workspace/transfer":{transfer:active}});renderAt(<AuthProvider hosted backend="rust"><WorkspaceTransferProvider><Probe/></WorkspaceTransferProvider></AuthProvider>,{route:"/companies/a"});await waitFor(()=>expect(screen.getByRole("button",{name:"Existing dialog confirm"})).not.toBeDisabled());await userEvent.click(screen.getByRole("button",{name:"Freeze mode"}));expect(screen.getByRole("button",{name:"Existing dialog confirm"})).toBeDisabled();expect(screen.getByRole("link",{name:"Read report"})).toHaveAttribute("href","/reports");
});
test("source notes use byte bounds and reject controls",()=>{expect(transferNote("enough source evidence")).toBe(true);expect(transferNote("short")).toBe(false);expect(transferNote("source\nother line")).toBe(false);expect(transferNote("æ".repeat(151),300)).toBe(false);});
test("staged target requires separately trusted key and frozen signed release before activation",async()=>{
 const staged={...frozen,mode:"staged",challenge:{version:1,destination:"Synthetic destination",destinationInstanceId:"target"}};
 mockFetch({"GET /api/me":me,"GET /api/workspace/transfer":{transfer:staged},"POST /api/workspace/transfer/activate":{transfer:active}});mount();await screen.findByRole("heading",{name:"Klargjort, endnu ikke aktivt mål"});
 expect(screen.getByRole("button",{name:"Gennemgå aktivering"})).toBeDisabled();
 await userEvent.type(screen.getByLabelText("Begrundelse"),"Owner verified target trust");await userEvent.type(screen.getByLabelText("Signeret frigivelse fra kilden"),"signed-envelope");await userEvent.click(screen.getByRole("checkbox",{name:"Kildens offentlige nøgle er konfigureret og kontrolleret separat"}));await userEvent.click(screen.getByRole("button",{name:"Gennemgå aktivering"}));
 const d=await screen.findByRole("dialog",{name:"Bekræft aktivering af målet"});expect(posts()).toHaveLength(0);await userEvent.click(within(d).getByRole("checkbox",{name:"Jeg bekræfter den gennemgåede handling"}));await userEvent.click(within(d).getByRole("button",{name:"Aktivér dette mål"}));await waitFor(()=>expect(posts()).toHaveLength(1));expect(JSON.parse(posts()[0][1].body)).toEqual({expectedStateHash:staged.stateHash,release:"signed-envelope",reason:"Owner verified target trust",confirm:true});
});
test("definite business refusal clears pending marker without automatic resend",async()=>{
 mockFetch({"GET /api/me":me,"GET /api/workspace/transfer":{transfer:active},"POST /api/workspace/transfer/freeze":{__error:{code:"business",message:"Current source refused"}}});mount();await freeze();await screen.findByText("Current source refused");expect(JSON.parse(sessionStorage.getItem("rentemester:uncertain-operations:v1")!)).toHaveLength(0);expect(posts()).toHaveLength(1);
});
test("status unavailable keeps native workspace controls fail closed",async()=>{
 mockFetch({"GET /api/me":me,"GET /api/workspace/transfer":{__error:{code:"unavailable",message:"Unavailable"}}});renderAt(<AuthProvider hosted backend="rust"><WorkspaceTransferProvider><Probe/></WorkspaceTransferProvider></AuthProvider>,{route:"/companies/a"});await waitFor(()=>expect((fetch as any).mock.calls.some(([url]:any[])=>url==="/api/workspace/transfer")).toBe(true));expect(screen.getByRole("button",{name:"Existing dialog confirm"})).toBeDisabled();
});
test("staged receipt mapping reviews retained outcome and maps only exact boundary to current user",async()=>{
 const row={receiptId:7,operation:"journal_post",keyHash:"k",payloadHash:"p",ledgerUuid:"ledger",sourcePrincipal:{kind:"user",subject:"source-owner"},createdAt:"now",expiresAt:"later",archiveSha256:"archive",restoreBoundaryId:3,resultAvailable:true,result:{ok:true,entryId:12},stateHash:"d".repeat(64),mapping:null};
 const mapped={...row,mapping:{targetUserId:user.id,actor:user.id,at:"now",reason:"Verified retained source outcome",archiveSha256:"archive",restoreBoundaryId:3}};
 mockFetch({"GET /api/me":me,"GET /api/workspace/transfer":{transfer:{...frozen,mode:"staged"}},"GET /api/companies/a/restore/receipts":{restoredReceipts:{rows:[row]}},"POST /api/companies/a/restore/receipts/reconcile":{restoredReceipts:{rows:[mapped]}}});mount();await screen.findByRole("heading",{name:"Importerede kvitteringer"});await userEvent.selectOptions(screen.getByLabelText("Virksomhed til kvitteringer"),"a");await screen.findByRole("article",{name:"Kvittering 7"});await userEvent.type(screen.getByLabelText("Begrundelse for tilknytning"),"Verified retained source outcome");await userEvent.click(screen.getByRole("button",{name:"Gennemgå tilknytning af kvittering 7"}));const d=await screen.findByRole("dialog",{name:"Bekræft tilknytning af kvittering"});expect(within(d).getByText(/entryId/)).toBeInTheDocument();expect(posts()).toHaveLength(0);await userEvent.click(within(d).getByRole("checkbox",{name:"Jeg bekræfter den gennemgåede handling"}));await userEvent.click(within(d).getByRole("button",{name:"Tilknyt kvittering til min bruger"}));await screen.findByText("Kvitteringen er tilknyttet. Ingen postering er udført.");expect(posts()).toHaveLength(1);expect(JSON.parse(posts()[0][1].body)).toEqual({receiptId:7,expectedStateHash:row.stateHash,reason:"Verified retained source outcome",confirm:true});
});
