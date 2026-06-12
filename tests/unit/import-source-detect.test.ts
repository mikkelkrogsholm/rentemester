// Tests: src/core/import/source-detect.ts — the cockpit file-import's
// source-recognition registry.
import { describe, expect, test } from "bun:test";
import {
  detectImportSource,
  IMPORT_MODULES,
  type ImportModule,
} from "../../src/core/import/source-detect";

const DINERO_CONTACTS_HEADER =
  "Kontaktnavn;Adresse;Postnummer;By;Landekode;CVR-nummer;EAN-nummer;" +
  "Telefon;E-mail;Att. person;Hjemmeside;Betalings metode;" +
  "Betalingsfrist i dage;Total salg;Total køb;Kontakttype";

describe("import source detection", () => {
  test("recognises a Dinero Kontakter export", () => {
    const csv =
      DINERO_CONTACTS_HEADER +
      "\nAcme ApS;Vej 1;1000;København;DK;12345678;;;;;;Netto;8;0;100;Company";
    const res = detectImportSource("Kontakter.csv", csv);
    expect(res.ok).toBe(true);
    if (res.ok) {
      expect(res.module.id).toBe("dinero-contacts");
      expect(res.module.system).toBe("Dinero");
      expect(res.module.dataType).toBe("contacts");
    }
  });

  test("recognises the header even with a UTF-8 BOM", () => {
    const res = detectImportSource("Kontakter.csv", "﻿" + DINERO_CONTACTS_HEADER);
    expect(res.ok).toBe(true);
  });

  test("an unrecognised file fails with the supported-formats list", () => {
    const res = detectImportSource("random.csv", "a,b,c\n1,2,3");
    expect(res.ok).toBe(false);
    if (!res.ok) {
      expect(res.errors[0]).toContain("ikke genkendt");
      expect(res.errors[0]).toContain("Dinero");
    }
  });

  test("a file that only partly matches the Dinero header is not recognised", () => {
    // `Kontakttype` missing — a bank CSV must not be mistaken for contacts.
    const res = detectImportSource(
      "x.csv",
      "Kontaktnavn;CVR-nummer;Beløb\nAcme;12345678;100",
    );
    expect(res.ok).toBe(false);
  });

  test("an empty file fails cleanly", () => {
    const res = detectImportSource("empty.csv", "   ");
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.errors[0]).toContain("tom");
  });

  test("every registered module has a unique id", () => {
    const ids = IMPORT_MODULES.map((m) => m.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  test("more than one matching module is reported as an ambiguous format", () => {
    const fake = (id: string): ImportModule => ({
      id,
      label: id,
      system: "Test",
      dataType: "contacts",
      detect: () => true,
    });
    const res = detectImportSource("x.csv", "anything", [
      fake("a"),
      fake("b"),
    ]);
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.errors[0]).toContain("flere kendte formater");
  });
});
