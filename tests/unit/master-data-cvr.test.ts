// Tests: src/core/master-data.ts (customerInputFromCvr / vendorInputFromCvr)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { cleanupDir } from "../helpers/cleanup";
import {
  createCustomer,
  customerInputFromCvr,
  vendorInputFromCvr,
} from "../../src/core/master-data";

const ENTITY = {
  cvrNummer: 12345678,
  reklamebeskyttet: false,
  navne: [{ navn: "Leverandør & Co ApS", periode: { gyldigTil: null } }],
  virksomhedsform: [{ virksomhedsformkode: 80, kortBeskrivelse: "ApS", langBeskrivelse: "Anpartsselskab", periode: { gyldigTil: null } }],
  beliggenhedsadresse: [
    { vejnavn: "Havnegade", husnummerFra: 9, postnummer: 5000, postdistrikt: "Odense C", kommune: { kommuneKode: 461 }, periode: { gyldigTil: null } },
  ],
  elektroniskPost: [{ kontaktoplysning: "faktura@leverandoer.dk", hemmelig: false, periode: { gyldigTil: null } }],
  virksomhedMetadata: {
    nyesteNavn: { navn: "Leverandør & Co ApS" },
    nyesteVirksomhedsform: { virksomhedsformkode: 80, kortBeskrivelse: "ApS", langBeskrivelse: "Anpartsselskab" },
    nyesteBeliggenhedsadresse: { vejnavn: "Havnegade", husnummerFra: 9, postnummer: 5000, postdistrikt: "Odense C", kommune: { kommuneKode: 461 } },
    sammensatStatus: "NORMAL",
  },
};

function fetchImpl(): typeof fetch {
  return (async () =>
    Response.json({ hits: { total: 1, hits: [{ _source: { Vrvirksomhed: ENTITY } }] } })) as unknown as typeof fetch;
}

function freshDb() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-md-cvr-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  return { root, db };
}

describe("customerInputFromCvr", () => {
  test("fills unset fields from CVR and a full postal address", async () => {
    const { root, db } = freshDb();
    const resolved = await customerInputFromCvr(db, "12345678", { name: "" }, { fetchImpl: fetchImpl(), username: "u", password: "p" });
    expect(resolved.ok).toBe(true);
    if (!resolved.ok) throw new Error("expected ok");
    expect(resolved.input.name).toBe("Leverandør & Co ApS");
    expect(resolved.input.address).toBe("Havnegade 9, 5000 Odense C");
    expect(resolved.input.vatOrCvr).toBe("DK12345678");
    expect(resolved.input.email).toBe("faktura@leverandoer.dk");

    // The resolved input must be a valid createCustomer payload.
    const created = createCustomer(db, resolved.input);
    expect(created.ok).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("explicit caller values always win over CVR data", async () => {
    const { root, db } = freshDb();
    const resolved = await customerInputFromCvr(
      db,
      "12345678",
      { name: "Mit eget kundenavn", email: "anden@adresse.dk" },
      { fetchImpl: fetchImpl(), username: "u", password: "p" },
    );
    expect(resolved.ok).toBe(true);
    if (!resolved.ok) throw new Error("expected ok");
    expect(resolved.input.name).toBe("Mit eget kundenavn");
    expect(resolved.input.email).toBe("anden@adresse.dk");
    // The field the caller left unset is still filled from CVR.
    expect(resolved.input.address).toBe("Havnegade 9, 5000 Odense C");

    db.close();
    cleanupDir(root);
  });

  test("reports the lookup error when the CVR number is unknown", async () => {
    const { root, db } = freshDb();
    const missing = (async () => Response.json({ hits: { total: 0, hits: [] } })) as unknown as typeof fetch;
    const resolved = await customerInputFromCvr(db, "99999999", { name: "" }, { fetchImpl: missing, username: "u", password: "p" });
    expect(resolved.ok).toBe(false);
    db.close();
    cleanupDir(root);
  });
});

describe("vendorInputFromCvr", () => {
  test("fills name, address and vatOrCvr from CVR", async () => {
    const { root, db } = freshDb();
    const resolved = await vendorInputFromCvr(db, "DK12345678", { name: "" }, { fetchImpl: fetchImpl(), username: "u", password: "p" });
    expect(resolved.ok).toBe(true);
    if (!resolved.ok) throw new Error("expected ok");
    expect(resolved.input.name).toBe("Leverandør & Co ApS");
    expect(resolved.input.address).toBe("Havnegade 9, 5000 Odense C");
    expect(resolved.input.vatOrCvr).toBe("DK12345678");
    db.close();
    cleanupDir(root);
  });
});
