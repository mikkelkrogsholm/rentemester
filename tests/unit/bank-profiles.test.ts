// Tests: src/core/bank-profiles.ts, src/core/bank.ts (Danske Bank CSV profile, #186)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { importBankCsv } from "../../src/core/bank";

import { cleanupDir } from "../helpers/cleanup";
function setup() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-bankprofile-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  return { root, db };
}

// A synthetic Danske Bank statement: ; delimiter, dd.mm.yyyy dates where both
// of the first two components are <= 12 (genuinely ambiguous for a generic
// parser), UTF-8 BOM.
const DANSKE_HEADER = "Dato;Rentedato;Tekst;Beløb;Valuta;Saldo;Afsender;Modtagerkonto;Besked;Arkivreference";
const DANSKE_ROWS = [
  "05.04.2026;06.04.2026;Overførsel;1.234,56;DKK;11.234,56;ACME ApS;3000111122223333;Faktura 100;ARK-1",
  "07.05.2026;08.05.2026;Kortbetaling;-200,00;DKK;11.034,56;;;Frokost;ARK-2",
];

describe("Danske Bank CSV profile (#186)", () => {
  test("imports ambiguous dd.mm.yyyy dates fully under --profile danske-bank", () => {
    const { root, db } = setup();
    const csv = join(root, "danske.csv");
    // Prepend a UTF-8 BOM so the encoding handling is exercised.
    writeFileSync(csv, "﻿" + [DANSKE_HEADER, ...DANSKE_ROWS].join("\r\n"));

    const result = importBankCsv(db, root, csv, { profile: "danske-bank" });
    expect(result.ok).toBe(true);
    expect(result.imported).toBe(2);
    expect(result.profile).toBe("danske-bank");

    const rows = db.query(
      "SELECT transaction_date, booking_date, text, amount FROM bank_transactions ORDER BY id ASC",
    ).all() as any[];
    // Dato -> transaction_date (posting), Rentedato -> booking_date (value).
    expect(rows[0].transaction_date).toBe("2026-04-05");
    expect(rows[0].booking_date).toBe("2026-04-06");
    expect(rows[0].amount).toBe(1234.56);
    expect(rows[1].transaction_date).toBe("2026-05-07");
    expect(rows[1].amount).toBe(-200);
    db.close();
    cleanupDir(root);
  });

  test("the same ambiguous file is rejected WITHOUT the profile", () => {
    const { root, db } = setup();
    const csv = join(root, "danske.csv");
    writeFileSync(csv, "﻿" + [DANSKE_HEADER, ...DANSKE_ROWS].join("\r\n"));

    const result = importBankCsv(db, root, csv);
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => e.includes("transactionDate"))).toBe(true);
    db.close();
    cleanupDir(root);
  });

  test("rejects an unknown profile name", () => {
    const { root, db } = setup();
    const csv = join(root, "x.csv");
    writeFileSync(csv, [DANSKE_HEADER, ...DANSKE_ROWS].join("\n"));
    const result = importBankCsv(db, root, csv, { profile: "no-such-bank" });
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => e.includes("unknown bank import profile"))).toBe(true);
    db.close();
    cleanupDir(root);
  });

  test("imports the committed synthetic Danske Bank sample", () => {
    const { root, db } = setup();
    const result = importBankCsv(db, root, "examples/bank-danske-bank.csv", { profile: "danske-bank" });
    expect(result.ok).toBe(true);
    expect((result.imported ?? 0)).toBeGreaterThan(0);
    db.close();
    cleanupDir(root);
  });
});
