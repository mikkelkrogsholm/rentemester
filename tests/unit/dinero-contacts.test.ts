// Tests: src/core/import/dinero-contacts.ts — Dinero Kontakter.csv import.
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { listCustomers, listVendors } from "../../src/core/master-data";
import { cleanupDir } from "../helpers/cleanup";
import {
  parseDineroContactsCsv,
  classifyContactRoles,
  importDineroContacts,
  type DineroContact,
} from "../../src/core/import/dinero-contacts";

const HEADER =
  "Kontaktnavn;Adresse;Postnummer;By;Landekode;CVR-nummer;EAN-nummer;Telefon;E-mail;Att. person;Hjemmeside;Betalings metode;Betalingsfrist i dage;Total salg;Total køb;Kontakttype";

// A representative export — note the UTF-8 BOM prefix and a trailing-comma street.
const CSV = "﻿" + [
  HEADER,
  "Leverandør ApS;Testvej 1, ;8000;Aarhus C;DK;12345678;;12345678;;;;Netto;8;0;5000;Company",
  "Kunde A/S;Kundevej 2;1000;København K;DK;87654321;;;kunde@eksempel.dk;;;Netto;14;9000;0;Company",
  "Foreign Inc;5 Main St;94000;San Francisco;US;;;;;;;Netto;8;0;700;Company",
  "Begge ApS;Beggevej 3;5000;Odense C;DK;11223344;;;begge@eksempel.dk;;;Netto;8;1200;800;Company",
  "Ingen Aktivitet ApS;Stillevej 4;9000;Aalborg;DK;55667788;;;;;;Netto;8;0;0;Company",
].join("\n") + "\n";

/**
 * A query-aware fake CVR endpoint: it reads the queried CVR number from the
 * Elasticsearch request body and returns an entity for it — rich contact
 * details for 12345678, a minimal record for any other CVR.
 */
function cvrFetchImpl(): typeof fetch {
  return (async (_url: unknown, init: { body: string }) => {
    const body = JSON.parse(init.body);
    const cvr = body?.query?.term?.["Vrvirksomhed.cvrNummer"];
    const rich = cvr === 12345678;
    const entity = {
      cvrNummer: cvr,
      reklamebeskyttet: false,
      navne: [{ navn: `CVR-navn ${cvr}`, periode: { gyldigTil: null } }],
      virksomhedsform: [
        { virksomhedsformkode: 80, kortBeskrivelse: "ApS", langBeskrivelse: "Anpartsselskab", periode: { gyldigTil: null } },
      ],
      elektroniskPost: rich ? [{ kontaktoplysning: "cvr@lev.dk", hemmelig: false, periode: { gyldigTil: null } }] : [],
      telefonNummer: rich ? [{ kontaktoplysning: "99999999", hemmelig: false, periode: { gyldigTil: null } }] : [],
      hjemmeside: rich ? [{ kontaktoplysning: "cvr-lev.dk", hemmelig: false, periode: { gyldigTil: null } }] : [],
      virksomhedMetadata: { nyesteNavn: { navn: `CVR-navn ${cvr}` }, sammensatStatus: "NORMAL" },
    };
    return Response.json({ hits: { total: 1, hits: [{ _source: { Vrvirksomhed: entity } }] } });
  }) as unknown as typeof fetch;
}

function freshDb() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-contacts-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  return { root, db };
}

describe("parseDineroContactsCsv", () => {
  test("parses rows, strips the BOM and composes a clean address", () => {
    const result = parseDineroContactsCsv(CSV);
    expect(result.ok).toBe(true);
    expect(result.contacts).toHaveLength(5);

    const vendor = result.contacts[0]!;
    expect(vendor.name).toBe("Leverandør ApS");
    // Trailing ", " on the street is dropped; postcode + city appended.
    expect(vendor.address).toBe("Testvej 1, 8000 Aarhus C");
    expect(vendor.phone).toBe("12345678");
  });

  test("normalises a Danish CVR and keeps a foreign VAT number as-is", () => {
    const contacts = parseDineroContactsCsv(CSV).contacts;
    const dk = contacts.find((c) => c.name === "Leverandør ApS")!;
    expect(dk.vatOrCvr).toBe("DK12345678");
    expect(dk.danishCvr).toBe("12345678");

    const foreign = contacts.find((c) => c.name === "Foreign Inc")!;
    expect(foreign.vatOrCvr).toBeNull();
    expect(foreign.danishCvr).toBeNull();
  });

  test("rejects a CSV without the Kontaktnavn column", () => {
    const result = parseDineroContactsCsv("Foo;Bar\na;b");
    expect(result.ok).toBe(false);
  });
});

describe("classifyContactRoles", () => {
  const base: DineroContact = {
    name: "X", address: null, vatOrCvr: null, countryCode: null, email: null,
    phone: null, website: null, eanNumber: null, attentionPerson: null,
    paymentTermsDays: null, totalSales: 0, totalPurchases: 0, contactType: null, danishCvr: null,
  };

  test("classifies by sales/purchase history", () => {
    expect(classifyContactRoles({ ...base, totalSales: 100 }, "vendor")).toEqual(["customer"]);
    expect(classifyContactRoles({ ...base, totalPurchases: 100 }, "vendor")).toEqual(["vendor"]);
    expect(classifyContactRoles({ ...base, totalSales: 5, totalPurchases: 9 }, "vendor")).toEqual(["customer", "vendor"]);
    // No history → the default role.
    expect(classifyContactRoles(base, "vendor")).toEqual(["vendor"]);
    expect(classifyContactRoles(base, "customer")).toEqual(["customer"]);
  });
});

describe("importDineroContacts", () => {
  test("imports contacts into customers/vendors and is idempotent on re-run", async () => {
    const { root, db } = freshDb();

    const first = await importDineroContacts(db, CSV);
    expect(first.ok).toBe(true);
    // Kunde A/S (sales) → customer; Begge ApS (both) → customer + vendor.
    expect(first.summary.customersCreated).toBe(2);
    // Leverandør ApS, Foreign Inc, Begge ApS (purchases) + Ingen Aktivitet (default vendor).
    expect(first.summary.vendorsCreated).toBe(4);

    // Re-import: every contact already exists → all skipped, nothing duplicated.
    const second = await importDineroContacts(db, CSV);
    expect(second.summary.customersCreated).toBe(0);
    expect(second.summary.vendorsCreated).toBe(0);
    expect(second.summary.skipped).toBe(6);
    expect(listCustomers(db).count).toBe(2);
    expect(listVendors(db).count).toBe(4);

    db.close();
    cleanupDir(root);
  });

  test("a plain import without --enrich-cvr makes no network call", async () => {
    const { root, db } = freshDb();
    const exploding = (async () => {
      throw new Error("network must not be touched without --enrich-cvr");
    }) as unknown as typeof fetch;

    const result = await importDineroContacts(db, CSV, { fetchImpl: exploding });
    expect(result.ok).toBe(true);
    expect(result.summary.enriched).toBe(0);

    db.close();
    cleanupDir(root);
  });

  test("--enrich-cvr fills empty fields from CVR while CSV values win", async () => {
    const { root, db } = freshDb();
    const result = await importDineroContacts(db, CSV, {
      enrichCvr: true,
      fetchImpl: cvrFetchImpl(),
      username: "u",
      password: "p",
    });
    expect(result.ok).toBe(true);
    // All four DK contacts get enriched; Foreign Inc (no danish CVR) does not.
    expect(result.summary.enriched).toBe(4);
    expect(result.summary.enrichmentFailures).toBe(0);

    const vendor = listVendors(db).rows.find((v) => v.vatOrCvr === "DK12345678")!;
    // CSV had no email → CVR fills it.
    expect(vendor.email).toBe("cvr@lev.dk");
    // CSV had a phone → it wins over the CVR phone.
    expect(vendor.phone).toBe("12345678");
    // CSV had no website → CVR fills it.
    expect(vendor.website).toBe("cvr-lev.dk");
    // CSV name always wins over the CVR-registered name.
    expect(vendor.name).toBe("Leverandør ApS");

    db.close();
    cleanupDir(root);
  });
});
