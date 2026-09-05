import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createCompany } from "../../src/core/company";
import { migrate, openDb } from "../../src/core/db";
import { applyDocumentPartyLink, planDocumentPartyLink } from "../../src/core/document-party-links";
import { ingestDocument } from "../../src/core/documents";
import { applyLegacyPartyMapping, planLegacyPartyMapping } from "../../src/core/legacy-party-mapping";
import { createVendor } from "../../src/core/master-data";
import { createParty, linkPartyRole } from "../../src/core/party-registry";
import { companyPaths } from "../../src/core/paths";
import { createSystemBackup } from "../../src/core/system-backups";
import { restoreSystemBackup } from "../../src/core/system-restore";
import { companyRootForSlug, initWorkspace } from "../../src/core/workspace";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import {
  applyVendorIdentityEnrichment,
  listVendorIdentityEnrichments,
  planVendorIdentityEnrichment,
} from "../../src/core/vendor-identity-enrichment";

const roots: string[] = [];

function setup(kind: "dk" | "non_eu") {
  const workspace = mkdtempSync(join(tmpdir(), "rm-vendor-identity-"));
  const inbox = mkdtempSync(join(tmpdir(), "rm-vendor-identity-source-"));
  roots.push(workspace, inbox);
  initWorkspace(workspace);
  const company = createCompany(workspace, { name: "Synthetic Company" });
  const companyRoot = companyRootForSlug(workspace, company.slug);
  const db = openDb(companyPaths(companyRoot).db);
  migrate(db);
  const name = kind === "dk" ? "Synthetic Supplier ApS" : "Outside Supplier Inc.";
  const address = kind === "dk" ? "Testvej 1, 1000 Testby" : "1 Example Street, New York";
  const identifier = kind === "dk" ? "DK11223344" : null;
  const vendor = createVendor(db, {
    name,
    address,
    vatOrCvr: identifier ?? undefined,
    notes: "Original operational note",
  });
  if (!vendor.ok) throw new Error(vendor.errors.join("; "));
  const sourcePath = join(inbox, `${kind}-invoice.txt`);
  writeFileSync(sourcePath, `immutable synthetic ${kind} invoice`);
  const document = ingestDocument(db, companyRoot, sourcePath, {
    source: "synthetic",
    documentType: "purchase_sale",
    issueDate: "2026-09-01",
    invoiceNo: `SYN-${kind}`,
    deliveryDescription: "Synthetic services",
    amountIncVat: 125,
    vatAmount: kind === "dk" ? 25 : 0,
    currency: "DKK",
    sender: {
      name,
      address,
      vatOrCvr: identifier ?? undefined,
      countryCode: kind === "dk" ? "DK" : "US",
      identifierKind: kind === "dk" ? "dk_cvr" : "non_eu",
    },
    recipient: { name: "Synthetic Company", address: "Buyer Road 1", vatOrCvr: "DK87654321" },
  });
  if (!document.ok) throw new Error(document.errors.join("; "));
  const control = openWorkspaceControlDb(workspace);
  const partyId = `party-${kind}`;
  createParty(control, {
    partyId,
    kind: "organization",
    name,
    identifiers: identifier ? [{ country: "DK", identifier, identifierKind: "dk_cvr" }] : [],
    source: "synthetic-document",
    observedAt: "2026-09-01T00:00:00.000Z",
    reviewAssertion: "Reviewed synthetic original",
    actor: "user:reviewer",
  });
  linkPartyRole(control, { partyId, companySlug: company.slug, role: "vendor", actor: "user:reviewer" });
  return {
    workspace,
    company,
    companyRoot,
    db,
    control,
    partyId,
    vendorId: vendor.vendorId,
    documentId: document.documentId,
    identity: {
      companySlug: company.slug,
      vendorId: vendor.vendorId,
      documentId: document.documentId,
      countryCode: kind === "dk" ? "DK" : "US",
      identifierKind: kind === "dk" ? "dk_cvr" as const : "non_eu" as const,
      identifier,
      reviewedReference: `review:${kind}:invoice`,
    },
  };
}

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

describe("#638 vendor identity enrichment", () => {
  test("rejects stale review and missing authority; preserves durable replay through restart and restore", () => {
    const ctx = setup("dk");
    const plan = planVendorIdentityEnrichment(ctx.db, ctx.companyRoot, ctx.identity);
    if (!plan.ok) throw new Error("expected plan");
    const request = { ...ctx.identity, planHash: plan.plan.planHash, idempotencyKey: "durable",
      confirm: true, actor: "agent:test", principal: "service:test" };
    for (const [override, code] of [
      [{ confirm: false }, "CONFIRMATION_REQUIRED"],
      [{ actor: "" }, "ACTOR_REQUIRED"],
      [{ principal: "" }, "PRINCIPAL_REQUIRED"],
    ] as const) {
      expect(applyVendorIdentityEnrichment(ctx.db, ctx.companyRoot, { ...request, ...override }))
        .toEqual({ ok: false, errors: [code] });
    }
    ctx.db.run("UPDATE vendors SET notes='Changed after review' WHERE id=?", [ctx.vendorId]);
    expect(applyVendorIdentityEnrichment(ctx.db, ctx.companyRoot, request))
      .toEqual({ ok: false, errors: ["PLAN_HASH_MISMATCH"] });
    expect(listVendorIdentityEnrichments(ctx.db, {})).toHaveLength(0);
    ctx.db.run("UPDATE vendors SET notes='Original operational note' WHERE id=?", [ctx.vendorId]);
    const applied = applyVendorIdentityEnrichment(ctx.db, ctx.companyRoot, request);
    expect(applied.ok).toBeTrue();
    const events = listVendorIdentityEnrichments(ctx.db, {});
    ctx.db.close();
    const reopened = openDb(companyPaths(ctx.companyRoot).db);
    expect(applyVendorIdentityEnrichment(reopened, ctx.companyRoot, request))
      .toEqual({ ...applied, idempotent: true });
    const backup = createSystemBackup(reopened, ctx.companyRoot, { createdAt: "2026-09-01T00:00:00.000Z" });
    expect(backup.ok).toBeTrue();
    const target = join(ctx.workspace, "restored-synthetic");
    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: target });
    expect(restored.ok).toBeTrue();
    const restoredDb = openDb(companyPaths(target).db);
    expect(listVendorIdentityEnrichments(restoredDb, {})).toEqual(events);
    expect(applyVendorIdentityEnrichment(restoredDb, target, request))
      .toEqual({ ...applied, idempotent: true });
    expect(() => restoredDb.run("DELETE FROM vendor_identity_enrichment_events")).toThrow("append-only");
    restoredDb.close();
    reopened.close();
    ctx.control.close();
  });
  for (const kind of ["dk", "non_eu"] as const) {
    test(`enriches an imported unresolved ${kind} vendor and enables the existing mapping flow`, () => {
      const ctx = setup(kind);
      const beforeDocument = ctx.db.query("SELECT * FROM documents WHERE id=?").get(ctx.documentId);
      const beforeCounts = {
        vendors: ctx.db.query("SELECT count(*) AS n FROM vendors").get(),
        journals: ctx.db.query("SELECT count(*) AS n FROM journal_entries").get(),
        vat: ctx.db.query("SELECT count(*) AS n FROM vat_validation_events").get(),
      };
      const plan = planVendorIdentityEnrichment(ctx.db, ctx.companyRoot, ctx.identity);
      expect(plan.ok).toBeTrue();
      if (!plan.ok) throw new Error("expected enrichment plan");
      const request = {
        ...ctx.identity,
        planHash: plan.plan.planHash,
        idempotencyKey: `enrich-${kind}`,
        confirm: true,
        actor: "agent:test",
        principal: "service:test",
      };
      const applied = applyVendorIdentityEnrichment(ctx.db, ctx.companyRoot, request);
      expect(applied).toMatchObject({ ok: true, idempotent: false });
      expect(applyVendorIdentityEnrichment(ctx.db, ctx.companyRoot, request)).toMatchObject({
        ok: true,
        id: applied.ok ? applied.id : undefined,
        idempotent: true,
      });
      expect(ctx.db.query("SELECT country_code,identifier_kind,identity_status,notes FROM vendors WHERE id=?")
        .get(ctx.vendorId)).toEqual({
        country_code: kind === "dk" ? "DK" : "US",
        identifier_kind: kind === "dk" ? "dk_cvr" : "non_eu",
        identity_status: "resolved",
        notes: "Original operational note",
      });

      const mappingInput = {
        companySlug: ctx.company.slug,
        legacyKind: "vendor" as const,
        legacyId: String(ctx.vendorId),
        partyId: ctx.partyId,
        role: "vendor" as const,
        documentId: ctx.documentId,
        reviewedLegacyReference: ctx.identity.reviewedReference,
      };
      const mapping = planLegacyPartyMapping(ctx.db, ctx.control, mappingInput);
      expect(mapping.ok).toBeTrue();
      if (!mapping.ok) throw new Error("expected mapping plan");
      expect(applyLegacyPartyMapping(ctx.db, ctx.control, {
        ...mappingInput,
        planHash: mapping.plan.planHash,
        idempotencyKey: `mapping-${kind}`,
        confirm: true,
        actor: "agent:test",
        principal: "service:test",
      }).ok).toBeTrue();
      const documentInput = {
        documentId: ctx.documentId,
        companySlug: ctx.company.slug,
        partyId: ctx.partyId,
        role: "vendor" as const,
        legacyKind: "vendor" as const,
        legacyId: String(ctx.vendorId),
        reviewedLegacyReference: ctx.identity.reviewedReference,
      };
      const documentPlan = planDocumentPartyLink(ctx.db, ctx.control, documentInput);
      expect(documentPlan.ok).toBeTrue();
      if (!documentPlan.ok) throw new Error("expected document-party plan");
      expect(applyDocumentPartyLink(ctx.db, ctx.control, {
        ...documentInput,
        planHash: documentPlan.plan.planHash,
        idempotencyKey: `document-${kind}`,
        confirm: true,
        actor: "agent:test",
        principal: "service:test",
      }).ok).toBeTrue();

      expect(ctx.db.query("SELECT * FROM documents WHERE id=?").get(ctx.documentId)).toEqual(beforeDocument);
      expect(ctx.db.query("SELECT count(*) AS n FROM vendors").get()).toEqual(beforeCounts.vendors);
      expect(ctx.db.query("SELECT count(*) AS n FROM journal_entries").get()).toEqual(beforeCounts.journals);
      expect(ctx.db.query("SELECT count(*) AS n FROM vat_validation_events").get()).toEqual(beforeCounts.vat);
      expect(listVendorIdentityEnrichments(ctx.db, { vendorId: ctx.vendorId })).toHaveLength(1);
      expect(() => ctx.db.run("UPDATE vendor_identity_enrichment_events SET actor='x'")).toThrow("append-only");
      expect(() => ctx.db.run("DELETE FROM vendor_identity_enrichment_events")).toThrow("append-only");
      ctx.control.close();
      ctx.db.close();
    });
  }

  test("fails closed for invented identity, wrong counterparty, tampered bytes, stale plan and conflicting retry", () => {
    const noId = setup("non_eu");
    expect(planVendorIdentityEnrichment(noId.db, noId.companyRoot, {
      ...noId.identity,
      identifier: "US-INVENTED-1",
    })).toEqual({ ok: false, errors: ["IDENTIFIER_INVENTION"] });
    noId.db.run("UPDATE documents SET sender_name='Different supplier' WHERE id=?", [noId.documentId]);
    expect(planVendorIdentityEnrichment(noId.db, noId.companyRoot, noId.identity))
      .toEqual({ ok: false, errors: ["NAME_OR_ADDRESS_MISMATCH"] });
    noId.control.close();
    noId.db.close();

    const exact = setup("dk");
    const plan = planVendorIdentityEnrichment(exact.db, exact.companyRoot, exact.identity);
    if (!plan.ok) throw new Error("expected exact plan");
    const stored = exact.db.query("SELECT stored_path FROM documents WHERE id=?").get(exact.documentId) as { stored_path: string };
    writeFileSync(stored.stored_path, "tampered bytes");
    expect(planVendorIdentityEnrichment(exact.db, exact.companyRoot, exact.identity))
      .toEqual({ ok: false, errors: ["EVIDENCE_INVALID"] });
    expect(applyVendorIdentityEnrichment(exact.db, exact.companyRoot, {
      ...exact.identity,
      planHash: plan.plan.planHash,
      idempotencyKey: "tampered",
      confirm: true,
      actor: "agent:test",
      principal: "service:test",
    })).toEqual({ ok: false, errors: ["EVIDENCE_INVALID"] });
    expect(exact.db.query("SELECT country_code FROM vendors WHERE id=?").get(exact.vendorId))
      .toEqual({ country_code: null });
    exact.control.close();
    exact.db.close();

    const retry = setup("dk");
    const retryPlan = planVendorIdentityEnrichment(retry.db, retry.companyRoot, retry.identity);
    if (!retryPlan.ok) throw new Error("expected retry plan");
    const request = {
      ...retry.identity,
      planHash: retryPlan.plan.planHash,
      idempotencyKey: "same-key",
      confirm: true,
      actor: "agent:test",
      principal: "service:test",
    };
    expect(applyVendorIdentityEnrichment(retry.db, retry.companyRoot, request).ok).toBeTrue();
    expect(applyVendorIdentityEnrichment(retry.db, retry.companyRoot, { ...request, reviewedReference: "changed" }))
      .toEqual({ ok: false, errors: ["IDEMPOTENCY_CONFLICT"] });
    retry.control.close();
    retry.db.close();
  });
});
