import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import { migrate } from "../../src/core/db";
import { ensurePurchaseVatPreflight, inspectPurchaseVatPreflight } from "../../src/core/purchase-vat-preflight";

const clock = { now: () => new Date("2026-08-01T10:00:00.000Z") };
function setup() {
  const db = new Database(":memory:"); migrate(db);
  db.run("INSERT INTO documents(id,source,sha256_hash,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status) VALUES(1,'test','preflight-document','DE123456789','DE','eu_vat','resolved')");
  return db;
}

describe("purchase VAT preflight", () => {
  test("non-EU identity is separate from purchase eligibility and shares booking's evidence gate", () => {
    const db = new Database(":memory:"); migrate(db);
    db.run("INSERT INTO companies (id,name,cvr,vat_period_type) VALUES (1,'Synthetic ApS','DK12345678','quarter')");
    db.run("INSERT INTO documents(id,source,sha256_hash,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status,recipient_vat_cvr,payload_json) VALUES(1,'test','non-eu-preflight','US-EIN-12-3456789','US','non_eu','resolved','DK12345678',?)", [JSON.stringify({})]);
    const missing = inspectPurchaseVatPreflight(db, 1, { clock });
    expect(missing).toMatchObject({ ok: false, classification: "NON_EU", wouldCallProvider: false });
    expect(missing.errors.join(" ")).toContain("reverse-charge wording");
    db.run("UPDATE documents SET payload_json=? WHERE id=1", [JSON.stringify({ reverseChargeWordingEvidence: { excerpt: "Reverse charge", location: "page 1" } })]);
    expect(inspectPurchaseVatPreflight(db, 1, { clock })).toMatchObject({ ok: true, classification: "NON_EU", errors: [] });
    db.close();
  });

  test("dry-run is pure, then reuses explicit fresh provider evidence", async () => {
    const db = setup(); let calls = 0;
    const dry = inspectPurchaseVatPreflight(db, 1, { clock });
    expect(dry).toMatchObject({ ok: false, classification: "EU", wouldCallProvider: true, cached: false });
    expect(db.query("SELECT COUNT(*) AS n FROM vat_validation_events").get()).toEqual({ n: 0 });
    const provider = { validate: async () => { calls++; return { status: "valid" as const, name: "Synthetic GmbH", address: "Berlin" }; } };
    const first = await ensurePurchaseVatPreflight(db, 1, provider, { clock, actor: "agent:test" });
    expect(first).toMatchObject({ ok: true, reusedEvidence: false });
    const second = await ensurePurchaseVatPreflight(db, 1, provider, { clock, actor: "agent:test" });
    expect(second).toMatchObject({ ok: true, reusedEvidence: true });
    expect(calls).toBe(1);
    expect(db.query("SELECT event_type, provider_status, actor, created_at FROM vat_validation_events ORDER BY id").all()).toEqual([
      { event_type: "provider_requested", provider_status: "requested", actor: "agent:test", created_at: "2026-08-01T10:00:00.000Z" },
      { event_type: "provider_result", provider_status: "valid", actor: "agent:test", created_at: "2026-08-01T10:00:00.000Z" },
      { event_type: "preflight_passed", provider_status: "valid", actor: "agent:test", created_at: "2026-08-01T10:00:00.000Z" },
    ]);
    db.close();
  });

  test("provider unavailability is a resumable, deduplicated exception", async () => {
    const db = setup();
    const provider = { validate: async () => ({ status: "unavailable" as const }) };
    const first = await ensurePurchaseVatPreflight(db, 1, provider, { clock, actor: "agent:test" });
    const second = await ensurePurchaseVatPreflight(db, 1, provider, { clock, actor: "agent:test" });
    expect(first.ok).toBe(false); expect(second.exceptionId).toBe(first.exceptionId);
    expect(db.query("SELECT COUNT(*) AS n FROM exceptions WHERE type='PURCHASE_VAT_PREFLIGHT'").get()).toEqual({ n: 1 });
    db.close();
  });
});
