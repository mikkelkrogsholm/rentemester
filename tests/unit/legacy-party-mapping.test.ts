import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createCompany } from "../../src/core/company";
import { migrate, openDb } from "../../src/core/db";
import { applyDocumentPartyLink, planDocumentPartyLink } from "../../src/core/document-party-links";
import { applyLegacyPartyMapping, inspectLegacyPartyMappings, planLegacyPartyMapping, supersedeLegacyPartyMapping } from "../../src/core/legacy-party-mapping";
import { createVendor, listVendors } from "../../src/core/master-data";
import { createParty, linkLegacyPartyReference, linkPartyRole } from "../../src/core/party-registry";
import { companyPaths } from "../../src/core/paths";
import { companyRootForSlug, initWorkspace } from "../../src/core/workspace";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";

const roots: string[] = [];

function setup() {
  const workspace = mkdtempSync(join(tmpdir(), "rm-legacy-party-"));
  roots.push(workspace);
  initWorkspace(workspace);
  const company = createCompany(workspace, { name: "Synthetic Company" });
  const companyRoot = companyRootForSlug(workspace, company.slug);
  const ledger = openDb(companyPaths(companyRoot).db);
  migrate(ledger);
  const vendor = createVendor(ledger, {
    name: "Outside Supplier Inc.",
    address: "1 Example Street, New York",
    countryCode: "US",
    identifierKind: "non_eu",
    notes: "Original operational note",
  });
  if (!vendor.ok) throw new Error(vendor.errors.join("; "));
  ledger.query(`INSERT INTO documents(document_no,sha256_hash,payload_json,upload_datetime,source,status,document_type,
    sender_name,sender_address,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)`).run(
      "DOC-638", "6".repeat(64), JSON.stringify({ source: "synthetic invoice" }), "2026-09-01T00:00:00.000Z",
      "synthetic", "inbox", "purchase_sale", "Outside Supplier Inc.", "1 Example Street, New York", null, "US", "non_eu", "resolved",
    );
  const control = openWorkspaceControlDb(workspace);
  const party = createParty(control, {
    partyId: "party-outside-supplier", kind: "organization", name: "Outside Supplier Inc.", identifiers: [],
    source: "reviewed-document", observedAt: "2026-09-01T00:00:00.000Z", reviewAssertion: "Reviewed original invoice",
    actor: "user:reviewer",
  });
  linkPartyRole(control, { partyId: party.partyId, companySlug: company.slug, role: "vendor", actor: "user:reviewer" });
  return { workspace, company, ledger, control, vendorId: String(vendor.vendorId), partyId: party.partyId };
}

afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

describe("#638 reviewed legacy contact mapping", () => {
  test("maps an identifier-less non-EU vendor and resolves its document without mutating source facts", () => {
    const { company, ledger, control, vendorId, partyId } = setup();
    const input = { companySlug: company.slug, legacyKind: "vendor" as const, legacyId: vendorId, partyId, role: "vendor" as const,
      documentId: 1, reviewedLegacyReference: "review:synthetic-invoice:1" };
    const beforeDocument = ledger.query("SELECT * FROM documents WHERE id=1").get();
    const beforeJournal = ledger.query("SELECT count(*) AS n FROM journal_entries").get();
    const beforeVat = ledger.query("SELECT count(*) AS n FROM vat_validation_events").get();
    const beforeVendors = ledger.query("SELECT count(*) AS n FROM vendors").get();
    const documentInput = { documentId: 1, companySlug: company.slug, partyId, role: "vendor" as const,
      legacyKind: "vendor" as const, legacyId: vendorId, reviewedLegacyReference: input.reviewedLegacyReference };
    expect(planDocumentPartyLink(ledger, control, documentInput)).toEqual({ ok:false, errors:["NO_IDENTIFIER_EVIDENCE"] });
    const plan = planLegacyPartyMapping(ledger, control, input);
    expect(plan.ok).toBeTrue();
    if (!plan.ok) throw new Error("expected mapping plan");
    const applied = applyLegacyPartyMapping(ledger, control, { ...input, planHash: plan.plan.planHash, idempotencyKey: "map-638-1",
      confirm: true, actor: "agent:test", principal: "service-638" });
    expect(applied).toMatchObject({ ok: true, idempotent: false });

    const documentPlan = planDocumentPartyLink(ledger, control, documentInput);
    expect(documentPlan.ok).toBeTrue();
    if (!documentPlan.ok) throw new Error("expected document party plan");
    expect(applyDocumentPartyLink(ledger, control, { ...documentInput, planHash: documentPlan.plan.planHash,
      idempotencyKey: "document-638-1", confirm: true, actor: "agent:test", principal: "service-638" })).toMatchObject({ ok: true });

    expect(ledger.query("SELECT * FROM documents WHERE id=1").get()).toEqual(beforeDocument);
    expect(ledger.query("SELECT count(*) AS n FROM journal_entries").get()).toEqual(beforeJournal);
    expect(ledger.query("SELECT count(*) AS n FROM vat_validation_events").get()).toEqual(beforeVat);
    expect(ledger.query("SELECT count(*) AS n FROM vendors").get()).toEqual(beforeVendors);
    expect(listVendors(ledger).rows.find((row) => String(row.id) === vendorId)?.notes).toBe("Original operational note");
    expect(inspectLegacyPartyMappings(control, { companySlug: company.slug })[0]?.contactSnapshot.notes).toBe("Original operational note");
    expect(() => control.run("UPDATE rm_legacy_party_mapping_events SET actor='user:other'")).toThrow("append-only");
    expect(() => control.run("DELETE FROM rm_legacy_party_mapping_events")).toThrow("append-only");
    control.close(); ledger.close();
  });

  test("fails closed for missing review, wrong company, identity mismatch and a conflicting prior mapping", () => {
    const { company, ledger, control, vendorId, partyId } = setup();
    const base = { companySlug: company.slug, legacyKind: "vendor" as const, legacyId: vendorId, partyId, role: "vendor" as const, documentId: 1 };
    expect(planLegacyPartyMapping(ledger, control, { ...base, reviewedLegacyReference: "" })).toEqual({ ok:false, errors:["EVIDENCE_REQUIRED"] });
    expect(planLegacyPartyMapping(ledger, control, { ...base, legacyId:`0${vendorId}`, reviewedLegacyReference:"reviewed" })).toEqual({ ok:false, errors:["CONTACT_KIND_INVALID"] });
    expect(planLegacyPartyMapping(ledger, control, { ...base, companySlug: "hidden-company", reviewedLegacyReference: "reviewed" })).toEqual({ ok:false, errors:["ROLE_MISMATCH"] });
    ledger.run("UPDATE documents SET sender_address='Different address' WHERE id=1");
    expect(planLegacyPartyMapping(ledger, control, { ...base, reviewedLegacyReference: "reviewed" })).toEqual({ ok:false, errors:["CONTACT_IDENTITY_MISMATCH"] });
    ledger.run("UPDATE documents SET sender_address='1 Example Street, New York' WHERE id=1");
    const other = createParty(control, { partyId:"party-other", kind:"organization", name:"Other", identifiers:[], source:"synthetic",
      observedAt:"2026-09-01T00:00:00.000Z", reviewAssertion:"reviewed", actor:"user:test" });
    linkPartyRole(control, { partyId:other.partyId, companySlug:company.slug, role:"vendor", actor:"user:test" });
    linkLegacyPartyReference(control, { partyId:other.partyId, companySlug:company.slug, legacyKind:"vendor", legacyId:vendorId, actor:"user:test" });
    expect(planLegacyPartyMapping(ledger, control, { ...base, reviewedLegacyReference:"reviewed" })).toEqual({ ok:false, errors:["CURRENT_STATE_CONFLICT"] });
    control.close(); ledger.close();
  });

  test("makes retries durable, rejects conflicting retries and requires explicit supersession", () => {
    const { company, ledger, control, vendorId, partyId } = setup();
    const input = { companySlug:company.slug, legacyKind:"vendor" as const, legacyId:vendorId, partyId, role:"vendor" as const,
      documentId:1, reviewedLegacyReference:"review:1" };
    const plan = planLegacyPartyMapping(ledger, control, input);
    if (!plan.ok) throw new Error("expected mapping plan");
    expect(applyLegacyPartyMapping(ledger, control, { ...input, planHash:plan.plan.planHash, idempotencyKey:"unauthorized",
      confirm:true, actor:"agent:test" })).toEqual({ ok:false, errors:["PRINCIPAL_REQUIRED"] });
    const request = { ...input, planHash:plan.plan.planHash, idempotencyKey:"map-retry", confirm:true, actor:"agent:test", principal:"service-638" };
    const first = applyLegacyPartyMapping(ledger, control, request);
    expect(first).toMatchObject({ ok:true, idempotent:false });
    ledger.run("UPDATE vendors SET notes='later note' WHERE id=?", Number(vendorId));
    expect(applyLegacyPartyMapping(ledger, control, request)).toMatchObject({ ok:true, idempotent:true, id:first.id });
    expect(applyLegacyPartyMapping(ledger, control, { ...request, planHash:"a".repeat(64) })).toEqual({ ok:false, errors:["IDEMPOTENCY_CONFLICT"] });
    expect(applyLegacyPartyMapping(ledger, control, { ...request, idempotencyKey:"new-key" })).toEqual({ ok:false, errors:["PLAN_HASH_MISMATCH"] });

    const supersede = { companySlug:company.slug, legacyKind:"vendor" as const, legacyId:vendorId, planHash:plan.plan.planHash,
      reason:"reviewed correction", idempotencyKey:"supersede-retry", confirm:true, actor:"agent:test", principal:"service-638" };
    const stopped = supersedeLegacyPartyMapping(control, supersede);
    expect(stopped).toMatchObject({ ok:true, idempotent:false });
    expect(supersedeLegacyPartyMapping(control, supersede)).toMatchObject({ ok:true, idempotent:true, id:stopped.id });
    expect(supersedeLegacyPartyMapping(control, { ...supersede, reason:"different" })).toEqual({ ok:false, errors:["IDEMPOTENCY_CONFLICT"] });
    expect(control.query("SELECT count(*) AS n FROM current_legacy_party_mappings").get()).toEqual({ n:0 });
    const history = inspectLegacyPartyMappings(control, { companySlug:company.slug, legacyKind:"vendor", legacyId:vendorId });
    expect(history.map((event) => event.version)).toEqual([1,2]);
    expect(history[1]?.priorEventHash).toBe(history[0]?.eventHash);
    control.close(); ledger.close();
  });
});
