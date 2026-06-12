// Tests: src/core/cvr.ts (CVR-register lookup, mapping and cache)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { cleanupDir } from "../helpers/cleanup";
import {
  normalizeCvrNumber,
  mapVirksomhed,
  lookupCvrCompany,
  getCachedCvrLookup,
} from "../../src/core/cvr";

/** A structurally-real `Vrvirksomhed` entity for a small ApS. */
const ENTITY = {
  cvrNummer: 12345678,
  reklamebeskyttet: false,
  navne: [{ navn: "Testvirksomhed ApS", periode: { gyldigFra: "2015-01-01", gyldigTil: null } }],
  virksomhedsform: [
    {
      virksomhedsformkode: 80,
      kortBeskrivelse: "ApS",
      langBeskrivelse: "Anpartsselskab",
      periode: { gyldigFra: "2015-01-01", gyldigTil: null },
    },
  ],
  virksomhedsstatus: [],
  beliggenhedsadresse: [
    {
      vejnavn: "Testvej",
      husnummerFra: 12,
      bogstavFra: "A",
      etage: "3",
      sidedoer: "tv",
      postnummer: 8000,
      postdistrikt: "Aarhus C",
      kommune: { kommuneKode: 751 },
      periode: { gyldigFra: "2015-01-01", gyldigTil: null },
    },
  ],
  hovedbranche: [
    {
      branchekode: "620100",
      branchetekst: "Computerprogrammering",
      periode: { gyldigFra: "2015-01-01", gyldigTil: null },
    },
  ],
  telefonNummer: [{ kontaktoplysning: "12345678", hemmelig: false, periode: { gyldigTil: null } }],
  elektroniskPost: [
    { kontaktoplysning: "skjult@eksempel.dk", hemmelig: true, periode: { gyldigTil: null } },
    { kontaktoplysning: "kontakt@testvirksomhed.dk", hemmelig: false, periode: { gyldigTil: null } },
  ],
  hjemmeside: [
    { kontaktoplysning: "www.testvirksomhed.dk", hemmelig: false, periode: { gyldigTil: null } },
  ],
  attributter: [
    { type: "REGNSKABSÅR_START", vaerdier: [{ vaerdi: "--07-01", periode: { gyldigTil: null } }] },
    { type: "REGNSKABSÅR_SLUT", vaerdier: [{ vaerdi: "--06-30", periode: { gyldigTil: null } }] },
    { type: "REVISION_FRAVALGT", vaerdier: [{ vaerdi: "true", periode: { gyldigTil: null } }] },
    { type: "KAPITAL", vaerdier: [{ vaerdi: "40000.00", periode: { gyldigTil: null } }] },
    { type: "KAPITALVALUTA", vaerdier: [{ vaerdi: "DKK", periode: { gyldigTil: null } }] },
  ],
  deltagerRelation: [
    {
      deltager: { navne: [{ navn: "Anders And", periode: { gyldigTil: null } }] },
      organisationer: [
        {
          hovedtype: "LEDELSESORGAN",
          organisationsNavn: [{ navn: "Direktion", periode: { gyldigTil: null } }],
          medlemsData: [
            {
              attributter: [
                { type: "FUNKTION", vaerdier: [{ vaerdi: "DIREKTØR", periode: { gyldigTil: null } }] },
              ],
            },
          ],
        },
      ],
    },
    {
      deltager: { navne: [{ navn: "Revisionsfirma P/S", periode: { gyldigTil: null } }] },
      organisationer: [
        { hovedtype: "REVISION", organisationsNavn: [{ navn: "Revision" }], medlemsData: [] },
      ],
    },
  ],
  virksomhedMetadata: {
    nyesteNavn: { navn: "Testvirksomhed ApS" },
    nyesteVirksomhedsform: { virksomhedsformkode: 80, kortBeskrivelse: "ApS", langBeskrivelse: "Anpartsselskab" },
    nyesteBeliggenhedsadresse: {
      vejnavn: "Testvej",
      husnummerFra: 12,
      bogstavFra: "A",
      etage: "3",
      sidedoer: "tv",
      postnummer: 8000,
      postdistrikt: "Aarhus C",
      kommune: { kommuneKode: 751 },
    },
    nyesteHovedbranche: { branchekode: "620100", branchetekst: "Computerprogrammering" },
    sammensatStatus: "NORMAL",
    stiftelsesDato: "2015-01-01",
    nyesteAarsbeskaeftigelse: { aar: 2024, antalAnsatte: 4 },
  },
};

function esResponse(entity: unknown): Response {
  return Response.json({ hits: { total: entity ? 1 : 0, hits: entity ? [{ _source: { Vrvirksomhed: entity } }] : [] } });
}

function freshDb() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-cvr-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  return { root, db };
}

describe("normalizeCvrNumber", () => {
  test("accepts plain, DK-prefixed and spaced 8-digit numbers", () => {
    expect(normalizeCvrNumber("12345678")).toBe("12345678");
    expect(normalizeCvrNumber("DK12345678")).toBe("12345678");
    expect(normalizeCvrNumber("dk12345678")).toBe("12345678");
    expect(normalizeCvrNumber(" 12 34 56 78 ")).toBe("12345678");
  });

  test("rejects non-CVR input", () => {
    expect(normalizeCvrNumber(null)).toBeNull();
    expect(normalizeCvrNumber("")).toBeNull();
    expect(normalizeCvrNumber("1234567")).toBeNull();
    expect(normalizeCvrNumber("123456789")).toBeNull();
    expect(normalizeCvrNumber("ABCDEFGH")).toBeNull();
  });
});

describe("mapVirksomhed", () => {
  test("maps a raw entity to the normalised snapshot shape", () => {
    const info = mapVirksomhed(ENTITY, "12345678");
    expect(info).toMatchObject({
      cvr: "12345678",
      name: "Testvirksomhed ApS",
      address: "Testvej 12A, 3. tv",
      postalCode: "8000",
      city: "Aarhus C",
      municipalityCode: 751,
      companyFormCode: 80,
      companyFormShort: "ApS",
      companyFormLong: "Anpartsselskab",
      status: "NORMAL",
      industryCode: "620100",
      industryText: "Computerprogrammering",
      phone: "12345678",
      website: "www.testvirksomhed.dk",
      startDate: "2015-01-01",
      fiscalYearStart: "--07-01",
      fiscalYearEnd: "--06-30",
      fiscalYearStartMonth: 7,
      auditWaived: true,
      shareCapital: 40000,
      shareCapitalCurrency: "DKK",
      employees: 4,
      advertisingProtected: false,
    });
  });

  test("drops secret contact rows and non-management participants", () => {
    const info = mapVirksomhed(ENTITY, "12345678");
    // The hemmelig=true email must not surface; the public one wins.
    expect(info.email).toBe("kontakt@testvirksomhed.dk");
    // Only LEDELSESORGAN members are management — the auditor is excluded.
    expect(info.management).toEqual([{ name: "Anders And", role: "DIREKTØR" }]);
  });
});

describe("lookupCvrCompany", () => {
  test("fetches, maps and caches a company; a second call serves the cache", async () => {
    const { root, db } = freshDb();
    let calls = 0;
    const fetchImpl = (async () => {
      calls += 1;
      return esResponse(ENTITY);
    }) as unknown as typeof fetch;

    const first = await lookupCvrCompany(db, "DK12345678", {
      fetchImpl,
      username: "u",
      password: "p",
    });
    expect(first.ok).toBe(true);
    expect(first.cached).toBe(false);
    expect(first.company?.name).toBe("Testvirksomhed ApS");
    expect(calls).toBe(1);

    const second = await lookupCvrCompany(db, "12345678", {
      fetchImpl,
      username: "u",
      password: "p",
    });
    expect(second.ok).toBe(true);
    expect(second.cached).toBe(true);
    expect(calls).toBe(1); // a fresh cache hit must not hit the network

    expect(getCachedCvrLookup(db, "12345678")?.company.fiscalYearStartMonth).toBe(7);

    db.close();
    cleanupDir(root);
  });

  test("fails gracefully without credentials and without a cache", async () => {
    const { root, db } = freshDb();
    const result = await lookupCvrCompany(db, "12345678", { username: "", password: "" });
    expect(result.ok).toBe(false);
    expect(result.errors[0]).toContain("CVR_USERNAME");
    db.close();
    cleanupDir(root);
  });

  test("reports a clear error when the CVR number is unknown", async () => {
    const { root, db } = freshDb();
    const fetchImpl = (async () => esResponse(null)) as unknown as typeof fetch;
    const result = await lookupCvrCompany(db, "87654321", {
      fetchImpl,
      username: "u",
      password: "p",
    });
    expect(result.ok).toBe(false);
    expect(result.errors[0]).toContain("87654321");
    db.close();
    cleanupDir(root);
  });

  test("falls back to a stale cache when a refresh fails", async () => {
    const { root, db } = freshDb();
    const ok = (async () => esResponse(ENTITY)) as unknown as typeof fetch;
    const seeded = await lookupCvrCompany(db, "12345678", {
      fetchImpl: ok,
      username: "u",
      password: "p",
      asOf: "2026-01-01T00:00:00.000Z",
    });
    expect(seeded.ok).toBe(true);

    // 60 days later the cache is stale; the network now fails — the stale
    // snapshot must still be returned rather than erroring.
    const failing = (async () => {
      throw new Error("network down");
    }) as unknown as typeof fetch;
    const result = await lookupCvrCompany(db, "12345678", {
      fetchImpl: failing,
      username: "u",
      password: "p",
      asOf: "2026-03-02T00:00:00.000Z",
    });
    expect(result.ok).toBe(true);
    expect(result.cached).toBe(true);
    expect(result.company?.name).toBe("Testvirksomhed ApS");

    db.close();
    cleanupDir(root);
  });
});
