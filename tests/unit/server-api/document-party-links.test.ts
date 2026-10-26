import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { writeFileSync } from "node:fs";
import { createParty, linkPartyRole } from "../../../src/core/party-registry";
import { openWorkspaceControlDb } from "../../../src/core/workspace-control";
import { seedAccounts, postJournalEntry } from "../../../src/core/ledger";
import {
  companyPaths,
  companyRootForSlug,
  config,
  get,
  makeWorkspace,
  migrate,
  openDb,
  rmSync,
} from "./_shared";

const slug = "acme-aps";
const identifier = "DK12345678";

function fixture() {
  const workspace = makeWorkspace("document-party-http", ["Acme ApS", "Hidden ApS"]);
  const ledger = openDb(companyPaths(companyRootForSlug(workspace, slug)).db);
  migrate(ledger);
  seedAccounts(ledger);
  const document = ledger.query(
    `INSERT INTO documents
      (document_no,sha256_hash,payload_json,upload_datetime,source,status,
       supplier_country_code,supplier_identifier_kind,sender_vat_cvr,sender_name,retain_until)
     VALUES(?,?,?,?,?,?,?,?,?,?,?) RETURNING id`,
  ).get("DOC-588", "a".repeat(64), "{}", "2026-08-30T00:00:00.000Z", "synthetic", "posted", "DK", "dk_cvr", identifier, "Evidence name", "2032-01-01") as { id: number };
  const posting = postJournalEntry(ledger, {
    transactionDate: "2026-08-30",
    text: "Immutable posted purchase",
    documentId: document.id,
    createdBy: "agent:synthetic",
    lines: [
      { accountNo: "3000", debitAmount: 100 },
      { accountNo: "2000", creditAmount: 100 },
    ],
  });
  if (!posting.ok) throw new Error(posting.errors.join("; "));
  const control = openWorkspaceControlDb(workspace);
  const visible = createParty(control, {
    partyId: "party-visible",
    kind: "organization",
    name: "Current canonical name",
    identifiers: [{ country: "DK", identifier, identifierKind: "dk_cvr" }],
    source: "synthetic-registry",
    observedAt: "2026-08-30T00:00:00.000Z",
    reviewAssertion: "reviewed synthetic identity",
    actor: "user:test",
  });
  linkPartyRole(control, { partyId: visible.partyId, companySlug: slug, role: "vendor", actor: "user:test" });
  const hidden = createParty(control, {
    partyId: "party-hidden",
    kind: "organization",
    name: "Hidden canonical party",
    source: "synthetic-registry",
    observedAt: "2026-08-30T00:00:00.000Z",
    reviewAssertion: "reviewed synthetic identity",
    actor: "user:test",
  });
  linkPartyRole(control, { partyId: hidden.partyId, companySlug: "hidden-aps", role: "vendor", actor: "user:test" });
  control.close();
  ledger.close();
  return { workspace, documentId: document.id };
}

function snapshot(workspace: string, documentId: number) {
  const db = openDb(companyPaths(companyRootForSlug(workspace, slug)).db);
  try {
    return {
      document: db.query("SELECT sha256_hash,payload_json,status FROM documents WHERE id=?").get(documentId),
      journal: db.query("SELECT id,entry_hash,previous_hash FROM journal_entries ORDER BY id").all(),
      eventCount: (db.query("SELECT count(*) AS n FROM document_party_link_events").get() as { n: number }).n,
    };
  } finally {
    db.close();
  }
}

async function post(workspace: string, path: string, body: unknown) {
  return get(config({ workspaceRoot: workspace }), path, {
    method: "POST",
    headers: { "content-type": "application/json", host: "127.0.0.1" },
    body: JSON.stringify(body),
  });
}

describe("#588 document-party HTTP contract", () => {
  test("plans read-only, confirms once, retries idempotently and preserves posted evidence", async () => {
    const { workspace, documentId } = fixture();
    try {
      const before = snapshot(workspace, documentId);
      const input = { documentId, role: "vendor", partyId: "party-visible", jurisdiction: "DK", identifierKind: "dk_cvr", identifier };

      const planned = await post(workspace, `/api/companies/${slug}/documents/party-links/plan`, input);
      expect(planned.status).toBe(200);
      expect(planned.body).toMatchObject({ ok: true, plan: { partyId: "party-visible", evidence: { kind: "exact_identifier" } } });
      expect(snapshot(workspace, documentId).eventCount).toBe(before.eventCount);

      const rejected = await post(workspace, `/api/companies/${slug}/documents/party-links/apply`, { ...input, planHash: planned.body.plan.planHash, idempotencyKey: "http-588-1", confirm: false });
      expect(rejected.status).toBe(400);
      expect(snapshot(workspace, documentId).eventCount).toBe(before.eventCount);

      const applied = await post(workspace, `/api/companies/${slug}/documents/party-links/apply`, { ...input, planHash: planned.body.plan.planHash, idempotencyKey: "http-588-1", confirm: true });
      expect(applied).toMatchObject({ status: 200, body: { ok: true, idempotent: false } });
      const retried = await post(workspace, `/api/companies/${slug}/documents/party-links/apply`, { ...input, planHash: planned.body.plan.planHash, idempotencyKey: "http-588-1", confirm: true });
      expect(retried).toMatchObject({ status: 200, body: { ok: true, idempotent: true } });

      const history = await get(config({ workspaceRoot: workspace }), `/api/companies/${slug}/documents/${documentId}/party-links`);
      const linked = await get(config({ workspaceRoot: workspace }), `/api/companies/${slug}/documents/party-links?status=linked`);
      expect(history.body.links).toHaveLength(1);
      expect(linked.body.links.map((row: any) => row.id)).toContain(documentId);
      const after = snapshot(workspace, documentId);
      expect(after.document).toEqual(before.document);
      expect(after.journal).toEqual(before.journal);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("company-scoped party search never reveals a party from another legal company", async () => {
    const { workspace } = fixture();
    try {
      const result = await get(config({ workspaceRoot: workspace }), `/api/companies/${slug}/workspace-parties?query=canonical`);
      expect(result.status).toBe(200);
      expect(result.body.rows.map((row: any) => row.partyId)).toEqual(["party-visible"]);
      expect(JSON.stringify(result.body)).not.toContain("party-hidden");
      expect(JSON.stringify(result.body)).not.toContain("Hidden canonical party");
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("HTTP exposes the same source-bound plan/apply contract",async()=>{const {workspace}=fixture();try{
    const root=companyRootForSlug(workspace,slug), path=join(root,"documents","originals","legacy-source.txt"), bytes=Buffer.from("synthetic source naming Foreign Merchant"); writeFileSync(path,bytes); const sha=createHash("sha256").update(bytes).digest("hex");
    const db=openDb(companyPaths(root).db); const doc=db.query("INSERT INTO documents(document_no,sha256_hash,stored_path,mime_type,payload_json,upload_datetime,source,status,document_type,retain_until) VALUES(?,?,?,?,?,?,?,?,?,?) RETURNING id").get("LEGACY-640",sha,path,"text/plain","{}","2026-09-03T00:00:00.000Z","synthetic","posted","purchase_sale","2032-01-01") as {id:number}; db.close();
    const registry=openWorkspaceControlDb(workspace); const party=createParty(registry,{partyId:"party-reviewed-http",kind:"organization",name:"Foreign Merchant",source:"synthetic",observedAt:"2026-09-03T00:00:00.000Z",reviewAssertion:"reviewed",actor:"user:test"}); linkPartyRole(registry,{partyId:party.partyId,companySlug:slug,role:"vendor",actor:"user:test"}); registry.close();
    const input={documentId:doc.id,partyId:party.partyId,role:"vendor",sourceReview:{observedName:"Foreign Merchant",jurisdiction:"US",identifierKind:"non_eu",sourceReference:"synthetic source",sourceLocation:"line 1",rationale:"reviewed immutable source"}};
    const planned=await post(workspace,`/api/companies/${slug}/documents/party-links/plan`,input); expect(planned).toMatchObject({status:200,body:{ok:true,plan:{documentSha256:sha,evidence:{kind:"reviewed_source_observation"}}}});
    const applied=await post(workspace,`/api/companies/${slug}/documents/party-links/apply`,{...input,planHash:planned.body.plan.planHash,idempotencyKey:"http-source-640",confirm:true}); expect(applied).toMatchObject({status:200,body:{ok:true,idempotent:false}});
  }finally{rmSync(workspace,{recursive:true,force:true});}});

  test("reaches the catalogued internal no-external-party route without changing evidence",async()=>{const {workspace}=fixture();try{const db=openDb(companyPaths(companyRootForSlug(workspace,slug)).db);const internal=db.query("INSERT INTO documents(document_no,sha256_hash,payload_json,upload_datetime,source,status,document_type,retain_until) VALUES(?,?,?,?,?,'stored','internal_voucher',?) RETURNING id").get("INTERNAL-588","b".repeat(64),"{}","2026-08-30T00:00:00.000Z","synthetic","2031-12-31") as {id:number};db.close();const before=snapshot(workspace,internal.id);const decided=await post(workspace,`/api/companies/${slug}/documents/internal-no-external-party`,{documentId:internal.id,reason:"Synthetic internal transfer",confirm:true,idempotencyKey:"internal-http"});expect(decided).toMatchObject({status:200,body:{ok:true,idempotent:false}});const after=snapshot(workspace,internal.id);expect(after.document).toEqual(before.document);expect(after.journal).toEqual(before.journal);}finally{rmSync(workspace,{recursive:true,force:true});}});
});
