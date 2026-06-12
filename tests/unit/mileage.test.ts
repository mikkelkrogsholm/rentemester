// Tests: src/core/mileage.ts (mileage log core)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import {
  createMileageEntry,
  listMileageEntries,
  buildMileagePeriodReport,
  exportMileageLog,
} from "../../src/core/mileage";
import { exportAuthorityPackage } from "../../src/core/authority-export";

import { cleanupDir } from "../helpers/cleanup";
function freshCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const companyRoot = join(root, "company");
  const db = openDb(ensureCompanyDirs(companyRoot).db);
  migrate(db);
  db.run(
    `INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy) VALUES (1, 'Rentemester ApS', 'DK12345678', 1, 'end-year')`,
  );
  return { root, companyRoot, db };
}

const validEntry = {
  tripDate: "2026-03-10",
  purpose: "Kundemøde i Aarhus",
  fromLocation: "København",
  toLocation: "Aarhus",
  kilometers: 312.5,
  vehicle: "AB 12 345",
  driver: "Mikkel Krogsholm",
  ratePerKm: 3.79,
  rateBasis: "Statens takst 2026, lav sats — bekræftet af bruger",
  rateSource: "https://skat.dk/satser",
};

describe("mileage log core", () => {
  test("rejects an entry that is missing required fields", () => {
    const { root, db } = freshCompany("rentemester-mileage-required-");

    expect(createMileageEntry(db, { ...validEntry, tripDate: "" }).ok).toBe(false);
    expect(createMileageEntry(db, { ...validEntry, tripDate: "2026-13-99" }).errors).toContain(
      "tripDate must be a valid YYYY-MM-DD date",
    );
    expect(createMileageEntry(db, { ...validEntry, purpose: "  " }).errors).toContain(
      "purpose is required",
    );
    expect(createMileageEntry(db, { ...validEntry, fromLocation: "" }).errors).toContain(
      "fromLocation is required",
    );
    expect(createMileageEntry(db, { ...validEntry, toLocation: "" }).errors).toContain(
      "toLocation is required",
    );
    expect(createMileageEntry(db, { ...validEntry, kilometers: 0 }).errors).toContain(
      "kilometers must be a positive number",
    );
    expect(createMileageEntry(db, { ...validEntry, kilometers: -5 }).errors).toContain(
      "kilometers must be a positive number",
    );
    expect(createMileageEntry(db, { ...validEntry, vehicle: "" }).errors).toContain(
      "vehicle is required",
    );
    expect(createMileageEntry(db, { ...validEntry, driver: "" }).errors).toContain(
      "driver is required",
    );
    expect(createMileageEntry(db, { ...validEntry, ratePerKm: 0 }).errors).toContain(
      "ratePerKm must be a positive number",
    );
    expect(createMileageEntry(db, { ...validEntry, rateBasis: "" }).errors).toContain(
      "rateBasis is required (user-supplied / source-backed)",
    );

    db.close();
    cleanupDir(root);
  });

  test("creates a mileage entry and lists it back deterministically", () => {
    const { root, db } = freshCompany("rentemester-mileage-create-");

    const created = createMileageEntry(db, validEntry);
    expect(created.ok).toBe(true);
    expect(created.mileageEntryId).toBe(1);
    expect(created.entryNo).toBe("MIL-2026-000001");
    expect(created.appliedRules).toEqual(["DK-MILEAGE-LOG-001"]);

    const second = createMileageEntry(db, { ...validEntry, tripDate: "2026-03-11" });
    expect(second.entryNo).toBe("MIL-2026-000002");

    const list = listMileageEntries(db);
    expect(list.ok).toBe(true);
    expect(list.count).toBe(2);
    // newest first, deterministic ordering
    expect(list.rows.map((r) => r.entryNo)).toEqual(["MIL-2026-000002", "MIL-2026-000001"]);
    const row = list.rows[1]!;
    expect(row.tripDate).toBe("2026-03-10");
    expect(row.kilometers).toBe(312.5);
    expect(row.ratePerKm).toBe(3.79);
    expect(row.amountBasis).toBe(1184.38); // 312.5 * 3.79 rounded to ore
    expect(row.rateBasis).toBe(validEntry.rateBasis);
    expect(row.rateSource).toBe(validEntry.rateSource);

    db.close();
    cleanupDir(root);
  });

  test("rejects an unknown rate currency mismatch and writes append-only entries", () => {
    const { root, db } = freshCompany("rentemester-mileage-immutable-");
    const created = createMileageEntry(db, validEntry);
    expect(created.ok).toBe(true);

    // Append-only: entries cannot be updated or deleted.
    expect(() => db.run("UPDATE mileage_entries SET kilometers = 1 WHERE id = 1")).toThrow();
    expect(() => db.run("DELETE FROM mileage_entries WHERE id = 1")).toThrow();

    db.close();
    cleanupDir(root);
  });

  test("period report sums kilometers and amount basis deterministically for a date range", () => {
    const { root, db } = freshCompany("rentemester-mileage-report-");

    createMileageEntry(db, { ...validEntry, tripDate: "2026-02-28", kilometers: 100, ratePerKm: 2 });
    createMileageEntry(db, { ...validEntry, tripDate: "2026-03-01", kilometers: 120.4, ratePerKm: 3.79 });
    createMileageEntry(db, { ...validEntry, tripDate: "2026-03-15", kilometers: 55.6, ratePerKm: 3.79 });
    createMileageEntry(db, { ...validEntry, tripDate: "2026-04-02", kilometers: 80, ratePerKm: 3.79 });

    const report = buildMileagePeriodReport(db, { from: "2026-03-01", to: "2026-03-31" });
    expect(report.ok).toBe(true);
    expect(report.from).toBe("2026-03-01");
    expect(report.to).toBe("2026-03-31");
    expect(report.appliedRules).toEqual(["DK-MILEAGE-LOG-001"]);
    expect(report.entryCount).toBe(2);
    expect(report.totalKilometers).toBe(176); // 120.4 + 55.6
    // 120.4*3.79 = 456.316 -> 456.32 ; 55.6*3.79 = 210.724 -> 210.72 ; sum = 667.04
    expect(report.totalAmountBasis).toBe(667.04);
    expect(report.entries.map((e) => e.tripDate)).toEqual(["2026-03-01", "2026-03-15"]);

    // Determinism: identical inputs produce identical output.
    const again = buildMileagePeriodReport(db, { from: "2026-03-01", to: "2026-03-31" });
    expect(again).toEqual(report);

    // Invalid range is rejected.
    const bad = buildMileagePeriodReport(db, { from: "2026-03-31", to: "2026-03-01" });
    expect(bad.ok).toBe(false);
    expect(bad.errors).toContain("from cannot be after to");

    db.close();
    cleanupDir(root);
  });

  test("mileage entries land in the audit log so they reach the authority export", () => {
    const { root, companyRoot, db } = freshCompany("rentemester-mileage-audit-");

    const created = createMileageEntry(db, { ...validEntry, tripDate: "2026-03-10" });
    expect(created.ok).toBe(true);

    const audit = db
      .query(
        "SELECT event_type, entity_type, entity_id, message, created_at FROM audit_log WHERE event_type = 'mileage_entry_create'",
      )
      .all() as Array<{ event_type: string; entity_type: string; entity_id: string; message: string; created_at: string }>;
    expect(audit.length).toBe(1);
    expect(audit[0]!.entity_type).toBe("mileage_entry");
    expect(audit[0]!.entity_id).toBe(String(created.mileageEntryId));
    expect(audit[0]!.message).toContain("MIL-2026-000001");
    // Backdate the audit row into the export window so the assertion is deterministic.
    db.exec("DROP TRIGGER IF EXISTS audit_log_no_update");
    db.run("UPDATE audit_log SET created_at = '2026-03-10 09:00:00' WHERE event_type = 'mileage_entry_create'");

    const authority = exportAuthorityPackage(db, companyRoot, {
      periodStart: "2026-03-01",
      periodEnd: "2026-03-31",
      outputDir: join(root, "exports"),
      requestedAt: "2026-05-17T03:00:00.000Z",
    });
    expect(authority.ok).toBe(true);
    const auditLog = JSON.parse(
      readFileSync(join(authority.exportDir!, "machine-readable", "audit-log.json"), "utf8"),
    ) as Array<{ eventType: string }>;
    expect(auditLog.some((row) => row.eventType === "mileage_entry_create")).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("exportMileageLog produces a deterministic period artifact for review and audit", () => {
    const { root, db } = freshCompany("rentemester-mileage-export-");

    createMileageEntry(db, { ...validEntry, tripDate: "2026-03-01", kilometers: 120.4, ratePerKm: 3.79 });
    createMileageEntry(db, { ...validEntry, tripDate: "2026-03-15", kilometers: 55.6, ratePerKm: 3.79 });
    createMileageEntry(db, { ...validEntry, tripDate: "2026-04-02", kilometers: 80, ratePerKm: 3.79 });

    const outDir = join(root, "mileage-export");
    const result = exportMileageLog(db, { from: "2026-03-01", to: "2026-03-31", outputDir: outDir });
    expect(result.ok).toBe(true);
    expect(result.entryCount).toBe(2);

    const json = JSON.parse(readFileSync(result.jsonPath!, "utf8"));
    expect(json.from).toBe("2026-03-01");
    expect(json.to).toBe("2026-03-31");
    expect(json.totalKilometers).toBe(176);
    expect(json.totalAmountBasis).toBe(667.04);
    expect(json.entries.length).toBe(2);
    expect(json.entries[0].entryNo).toBe("MIL-2026-000001");

    const csv = readFileSync(result.csvPath!, "utf8");
    expect(csv.split("\n")[0]).toBe(
      "entry_no,trip_date,purpose,from_location,to_location,kilometers,vehicle,driver,rate_per_km,amount_basis,rate_basis,rate_source",
    );
    expect(csv).toContain("MIL-2026-000001");

    // Deterministic: re-running yields byte-identical artifacts.
    const outDir2 = join(root, "mileage-export-2");
    const result2 = exportMileageLog(db, { from: "2026-03-01", to: "2026-03-31", outputDir: outDir2 });
    expect(readFileSync(result2.csvPath!, "utf8")).toBe(csv);
    expect(readFileSync(result2.jsonPath!, "utf8")).toBe(readFileSync(result.jsonPath!, "utf8"));

    db.close();
    cleanupDir(root);
  });
});
