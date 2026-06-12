// Tests: src/core/bank.ts (running-balance continuity, #189)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { importBankCsv } from "../../src/core/bank";
import { listExceptions } from "../../src/core/exceptions";

import { cleanupDir } from "../helpers/cleanup";
function setup() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-bankbal-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  return { root, db };
}

const HEADER = "Dato;Rentedato;Tekst;Beløb;Valuta;Saldo;Afsender;Modtagerkonto;Besked;Arkivreference;Kundereference";

// A clean statement: each Saldo equals the previous Saldo + this row's Beløb.
const CLEAN_ROWS = [
  "01.02.2026;01.02.2026;Indbetaling;1.000,00;DKK;1.000,00;ACME ApS;3000;Faktura 1;ARK-1;KND-1",
  "02.03.2026;02.03.2026;Kortbetaling;-250,00;DKK;750,00;;;Frokost;ARK-2;KND-2",
  "03.04.2026;03.04.2026;Gebyr;-50,00;DKK;700,00;;;Gebyr;ARK-3;KND-3",
];

describe("running-balance continuity (#189)", () => {
  test("a clean statement passes silently with first/last balance reported", () => {
    const { root, db } = setup();
    const csv = join(root, "clean.csv");
    writeFileSync(csv, "﻿" + [HEADER, ...CLEAN_ROWS].join("\r\n"));

    const result = importBankCsv(db, root, csv, { profile: "danske-bank" });
    expect(result.ok).toBe(true);
    expect(result.imported).toBe(3);
    expect(result.balanceWarnings ?? []).toEqual([]);
    expect(result.firstBalance).toBe(1000);
    expect(result.lastBalance).toBe(700);

    const exceptions = listExceptions(db, { status: "open" });
    expect(exceptions.rows.some((e) => e.type === "BANK_BALANCE_GAP")).toBe(false);

    db.close();
    cleanupDir(root);
  });

  test("a statement with a removed row is flagged as a balance gap", () => {
    const { root, db } = setup();
    const csv = join(root, "gap.csv");
    // The middle -250 row is removed: the 700,00 balance no longer follows
    // from 1.000,00 by the remaining +(-50) amount.
    writeFileSync(csv, "﻿" + [HEADER, CLEAN_ROWS[0], CLEAN_ROWS[2]].join("\r\n"));

    const result = importBankCsv(db, root, csv, { profile: "danske-bank" });
    // The import still succeeds — a partial export is legitimate.
    expect(result.ok).toBe(true);
    expect(result.imported).toBe(2);
    expect((result.balanceWarnings ?? []).length).toBeGreaterThan(0);

    const exceptions = listExceptions(db, { status: "open" });
    const gap = exceptions.rows.find((e) => e.type === "BANK_BALANCE_GAP");
    expect(gap).toBeDefined();

    db.close();
    cleanupDir(root);
  });

  // A real Danske Bank export is ordered newest-first: the latest transaction
  // is the first data row. Within a single date the file therefore lists
  // same-day rows in the reverse of their true intra-day sequence. The
  // continuity check must reconstruct the true order before walking, otherwise
  // same-day clusters raise spurious "balance break" warnings (#191).
  //
  // True chronological sequence (each Saldo = previous + Beløb):
  //   2026-02-26  +10.000,00 -> 10.000,00
  //   2026-02-27       -2,00 ->  9.998,00   (fee)
  //   2026-02-27   -5.334,00 ->  4.664,00   (VAT payment)
  //   2026-02-28     +100,00 ->  4.764,00
  const NEWEST_FIRST_ROWS = [
    "28.02.2026;28.02.2026;Indbetaling;100,00;DKK;4.764,00;ACME ApS;3000;Faktura 9;ARK-9;KND-9",
    "27.02.2026;27.02.2026;Momsbetaling;-5.334,00;DKK;4.664,00;;;Moms;ARK-8;KND-8",
    "27.02.2026;27.02.2026;Gebyr;-2,00;DKK;9.998,00;;;Gebyr;ARK-7;KND-7",
    "26.02.2026;26.02.2026;Indbetaling;10.000,00;DKK;10.000,00;ACME ApS;3000;Faktura 6;ARK-6;KND-6",
  ];

  test("a newest-first export with same-day rows raises zero balance warnings (#191)", () => {
    const { root, db } = setup();
    const csv = join(root, "newest-first.csv");
    writeFileSync(csv, "﻿" + [HEADER, ...NEWEST_FIRST_ROWS].join("\r\n"));

    const result = importBankCsv(db, root, csv, { profile: "danske-bank" });
    expect(result.ok).toBe(true);
    expect(result.imported).toBe(4);
    // The two 2026-02-27 rows chain cleanly once true intra-day order is
    // reconstructed; the walk must not flag them.
    expect(result.balanceWarnings ?? []).toEqual([]);

    const exceptions = listExceptions(db, { status: "open" });
    expect(exceptions.rows.some((e) => e.type === "BANK_BALANCE_GAP")).toBe(false);

    db.close();
    cleanupDir(root);
  });

  test("a newest-first export with a removed same-day row is still flagged (#191)", () => {
    const { root, db } = setup();
    const csv = join(root, "newest-first-gap.csv");
    // The -5.334,00 VAT row is removed: the surviving 2026-02-27 fee row's
    // 9.998,00 balance can no longer chain to the 2026-02-28 row's 4.764,00.
    writeFileSync(csv, "﻿" + [
      HEADER,
      NEWEST_FIRST_ROWS[0],
      NEWEST_FIRST_ROWS[2],
      NEWEST_FIRST_ROWS[3],
    ].join("\r\n"));

    const result = importBankCsv(db, root, csv, { profile: "danske-bank" });
    expect(result.ok).toBe(true);
    expect(result.imported).toBe(3);
    expect((result.balanceWarnings ?? []).length).toBeGreaterThan(0);

    const exceptions = listExceptions(db, { status: "open" });
    expect(exceptions.rows.some((e) => e.type === "BANK_BALANCE_GAP")).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("a generic CSV without a balance column is not balance-checked", () => {
    const { root, db } = setup();
    const csv = join(root, "generic.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,Payment,-100,DKK,R-1",
    ].join("\n"));

    const result = importBankCsv(db, root, csv);
    expect(result.ok).toBe(true);
    expect(result.balanceWarnings ?? []).toEqual([]);
    expect(result.firstBalance).toBeUndefined();

    db.close();
    cleanupDir(root);
  });
});
