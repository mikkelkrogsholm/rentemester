// Tests: src/core/company.ts (syncCompanyFromCvr)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { initialiseCompanyVolume, getCompanySettings, syncCompanyFromCvr } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";

import { cleanupDir } from "../helpers/cleanup";
const ENTITY = {
  cvrNummer: 12345678,
  reklamebeskyttet: false,
  navne: [{ navn: "Synket ApS", periode: { gyldigTil: null } }],
  virksomhedsform: [
    { virksomhedsformkode: 80, kortBeskrivelse: "ApS", langBeskrivelse: "Anpartsselskab", periode: { gyldigTil: null } },
  ],
  beliggenhedsadresse: [
    { vejnavn: "Bredgade", husnummerFra: 5, postnummer: 1260, postdistrikt: "København K", kommune: { kommuneKode: 101 }, periode: { gyldigTil: null } },
  ],
  hovedbranche: [{ branchekode: "620100", branchetekst: "Computerprogrammering", periode: { gyldigTil: null } }],
  attributter: [
    { type: "REGNSKABSÅR_START", vaerdier: [{ vaerdi: "--07-01", periode: { gyldigTil: null } }] },
    { type: "REGNSKABSÅR_SLUT", vaerdier: [{ vaerdi: "--06-30", periode: { gyldigTil: null } }] },
    { type: "REVISION_FRAVALGT", vaerdier: [{ vaerdi: "true", periode: { gyldigTil: null } }] },
  ],
  virksomhedMetadata: {
    nyesteNavn: { navn: "Synket ApS" },
    nyesteVirksomhedsform: { virksomhedsformkode: 80, kortBeskrivelse: "ApS", langBeskrivelse: "Anpartsselskab" },
    nyesteBeliggenhedsadresse: { vejnavn: "Bredgade", husnummerFra: 5, postnummer: 1260, postdistrikt: "København K", kommune: { kommuneKode: 101 } },
    nyesteHovedbranche: { branchekode: "620100", branchetekst: "Computerprogrammering" },
    sammensatStatus: "NORMAL",
    stiftelsesDato: "2018-03-01",
  },
};

function esResponse(): Response {
  return Response.json({ hits: { total: 1, hits: [{ _source: { Vrvirksomhed: ENTITY } }] } });
}

describe("syncCompanyFromCvr", () => {
  test("writes CVR stamdata onto the companies row and reports a fiscal-year mismatch", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-company-cvr-"));
    initialiseCompanyVolume(root, { name: "Placeholder", cvr: "DK12345678" });
    const db = openDb(companyPaths(root).db);

    const fetchImpl = (async () => esResponse()) as unknown as typeof fetch;
    const result = await syncCompanyFromCvr(db, { fetchImpl, username: "u", password: "p" });

    expect(result.ok).toBe(true);
    expect(result.cvr).toBe("12345678");
    expect(result.updatedFields).toContain("name");
    expect(result.updatedFields).toContain("address");
    expect(result.updatedFields).toContain("industryText");
    // The fiscal year is locked — sync must only report the mismatch.
    expect(result.fiscalYearStartMonth).toEqual({ current: 1, cvr: 7, matches: false });

    const settings = getCompanySettings(db);
    expect(settings.name).toBe("Synket ApS");
    expect(settings.address).toBe("Bredgade 5");
    expect(settings.postalCode).toBe("1260");
    expect(settings.city).toBe("København K");
    expect(settings.companyForm).toBe("ApS");
    expect(settings.industryText).toBe("Computerprogrammering");
    expect(settings.cvrStatus).toBe("NORMAL");
    expect(settings.auditWaived).toBe(true);
    expect(settings.cvrSyncedAt).not.toBeNull();
    // The fiscal-year config itself must be untouched.
    expect(settings.fiscalYearStartMonth).toBe(1);

    const audit = db
      .query("SELECT COUNT(*) AS n FROM audit_log WHERE event_type = 'company_cvr_sync'")
      .get() as { n: number };
    expect(audit.n).toBe(1);

    db.close();
    cleanupDir(root);
  });

  test("fails clearly when the company has no CVR number", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-company-nocvr-"));
    initialiseCompanyVolume(root, { name: "Uden CVR" });
    const db = openDb(companyPaths(root).db);

    const result = await syncCompanyFromCvr(db, { username: "u", password: "p" });
    expect(result.ok).toBe(false);
    expect(result.errors[0]).toContain("CVR-nummer");

    db.close();
    cleanupDir(root);
  });
});
