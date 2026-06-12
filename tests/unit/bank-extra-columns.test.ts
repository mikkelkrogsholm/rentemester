// Tests: src/core/bank.ts, src/core/bank-suggest-matches.ts (extra columns, #188)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { issueInvoice } from "../../src/core/issued-invoices";
import { importBankCsv } from "../../src/core/bank";
import { suggestBankMatches } from "../../src/core/bank-suggest-matches";

import { cleanupDir } from "../helpers/cleanup";
function invoicePayload(overrides: Record<string, unknown> = {}) {
  return {
    invoiceType: "full",
    vatTreatment: "standard",
    issueDate: "2026-05-16",
    invoiceNumber: "2026-0001",
    seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
    buyer: { name: "Kunde A/S", address: "Købervej 9, 8000 Aarhus C", vatOrCvr: "DK87654321" },
    lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
    totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
    currency: "DKK",
    dueDate: "2026-06-15",
    ...overrides,
  };
}

describe("bank import extra columns (#188)", () => {
  test("the danske-bank profile persists counterparty, message, references and raw_json", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bankcols-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const csv = join(root, "danske.csv");
    writeFileSync(csv, "﻿" + [
      "Dato;Rentedato;Tekst;Beløb;Valuta;Saldo;Afsender;Modtagerkonto;Besked;Arkivreference;Kundereference",
      "05.04.2026;06.04.2026;Overførsel;1.234,56;DKK;11.234,56;ACME ApS;3000111122223333;Faktura 100;ARK-1;KND-9",
    ].join("\r\n"));

    expect(importBankCsv(db, root, csv, { profile: "danske-bank" }).ok).toBe(true);
    const row = db.query(
      "SELECT counterparty_name, counterparty_account, message, archive_reference, customer_reference, raw_json FROM bank_transactions ORDER BY id ASC LIMIT 1",
    ).get() as any;
    expect(row.counterparty_name).toBe("ACME ApS");
    expect(row.counterparty_account).toBe("3000111122223333");
    expect(row.message).toBe("Faktura 100");
    expect(row.archive_reference).toBe("ARK-1");
    expect(row.customer_reference).toBe("KND-9");
    expect(JSON.parse(row.raw_json).Tekst).toBe("Overførsel");

    db.close();
    cleanupDir(root);
  });

  test("a counterparty/message-identified payment now produces a corroborated suggestion text+reference missed", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bankcols-match-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    // Customer named "Hjørnegaard Tømrer ApS".
    expect(issueInvoice(db, root, invoicePayload({
      invoiceNumber: "2026-0001",
      buyer: { name: "Hjørnegaard Tømrer ApS", address: "Tømrervej 4, 5000 Odense", vatOrCvr: "DK55667788" },
    })).ok).toBe(true);

    // A Danske Bank row whose generic `text` is just "Overførsel" (no signal)
    // but whose Afsender (counterparty) and Besked (message) name the customer
    // and the invoice. Without the extra columns this row matched on amount
    // only and stayed below threshold.
    const csv = join(root, "danske.csv");
    writeFileSync(csv, "﻿" + [
      "Dato;Rentedato;Tekst;Beløb;Valuta;Saldo;Afsender;Modtagerkonto;Besked;Arkivreference;Kundereference",
      "05.04.2026;06.04.2026;Overførsel;1.250,00;DKK;1.250,00;Hjørnegaard Tømrer ApS;3000111122223333;Faktura 2026-0001;ARK-7;KND-7",
    ].join("\r\n"));
    expect(importBankCsv(db, root, csv, { profile: "danske-bank" }).ok).toBe(true);

    const result = suggestBankMatches(db, {});
    expect(result.ok).toBe(true);
    const row = result.rows[0];
    expect(row.suggestions.length).toBeGreaterThan(0);
    const top = row.suggestions[0];
    expect(top.kind).toBe("issued_invoice");
    expect(top.invoiceNo).toBe("2026-0001");
    // The suggestion is corroborated (crosses threshold) and not flagged as a
    // low-confidence amount-only match.
    expect(top.confidence).toBeGreaterThanOrEqual(0.5);
    expect(top.reasons.some((r: string) => r.includes("low confidence"))).toBe(false);

    db.close();
    cleanupDir(root);
  });

  test("extra columns introduce no false positive on an amount-only tie", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bankcols-fp-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    expect(issueInvoice(db, root, invoicePayload({
      invoiceNumber: "2026-0001",
      buyer: { name: "Alfa Bogforing ApS", address: "Alfavej 1", vatOrCvr: "DK11111111" },
    })).ok).toBe(true);
    expect(issueInvoice(db, root, invoicePayload({
      invoiceNumber: "2026-0002",
      buyer: { name: "Beta Revision ApS", address: "Betavej 2", vatOrCvr: "DK22222222" },
    })).ok).toBe(true);

    // A Danske Bank deposit equal to both invoice balances, with no invoice
    // number and a counterparty/message that names neither customer.
    const csv = join(root, "danske.csv");
    writeFileSync(csv, "﻿" + [
      "Dato;Rentedato;Tekst;Beløb;Valuta;Saldo;Afsender;Modtagerkonto;Besked;Arkivreference;Kundereference",
      "05.04.2026;06.04.2026;Overførsel;1.250,00;DKK;1.250,00;Ukendt Indbetaler;3000999988887777;Diverse;ARK-X;KND-X",
    ].join("\r\n"));
    expect(importBankCsv(db, root, csv, { profile: "danske-bank" }).ok).toBe(true);

    const result = suggestBankMatches(db, {});
    expect(result.rows[0].suggestions).toHaveLength(0);

    db.close();
    cleanupDir(root);
  });
});
