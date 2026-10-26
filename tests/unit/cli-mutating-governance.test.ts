// Tests: src/cli-actor.ts (MUTATING_COMMANDS governance-klassen) og
// src/cli-meta/helpers.ts (renderGlobalUsage-grupperingen).
//
// Audit 2026-06-11, AGENT-1 + AGENT-2:
//   - `gdpr forget` (kanonisk navn) deler runEraser med `gdpr erase`
//     (legacy-alias), men kun aliaset stod i MUTATING_COMMANDS. Den kanoniske
//     kommando kørte derfor UDEN actor-gate og blev listet som read-only i
//     den globale hjælp.
//   - `company sync-cvr` skriver CVR-stamdata til company-tabellen (MCP-
//     pendanten company_sync_cvr er write-reversible + confirm-gated), men
//     stod heller ikke i MUTATING_COMMANDS.
//
// Kontrakten her: en kommando og dens alias SKAL have samme governance-
// klasse, og hjælpe-grupperingen skal udledes af MUTATING_COMMANDS (det gør
// renderGlobalUsage allerede — disse tests låser det fast for de to fund).
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { MUTATING_COMMANDS, enforceMutationActorPolicy, mutationPolicyScope } from "../../src/cli-actor";
import { renderGlobalUsage, COMMAND_SPECS } from "../../src/cli-meta";

/**
 * Kendte alias-par: [legacy/alias, kanonisk navn]. Der findes ingen formel
 * alias-tabel i koden (aliasser registreres som separate dispatch.on-kald,
 * jf. src/cli/gdpr.ts), så parrene vedligeholdes her. Tilføj nye par når et
 * alias indføres — testen håndhæver at begge navne altid lander i samme
 * governance-klasse.
 */
const COMMAND_ALIASES: Array<[alias: string, canonical: string]> = [
  ["gdpr erase", "gdpr forget"],
];

describe("MUTATING_COMMANDS governance-klasser (audit AGENT-1/AGENT-2)", () => {
  test("en kommando og dens alias er altid i samme governance-klasse", () => {
    for (const [alias, canonical] of COMMAND_ALIASES) {
      const aliasMutating = MUTATING_COMMANDS.has(alias);
      const canonicalMutating = MUTATING_COMMANDS.has(canonical);
      expect(
        aliasMutating === canonicalMutating,
        `'${alias}' (mutating=${aliasMutating}) og '${canonical}' (mutating=${canonicalMutating}) ` +
          "er i forskellige governance-klasser — et alias og dets kanoniske navn deler " +
          "implementering og SKAL gates ens",
      ).toBe(true);
    }
  });

  test("gdpr forget er en actor-gated mutation (samme klasse som gdpr erase)", () => {
    expect(MUTATING_COMMANDS.has("gdpr erase")).toBe(true);
    expect(MUTATING_COMMANDS.has("gdpr forget")).toBe(true);
  });

  test("company sync-cvr er en actor-gated mutation (skriver CVR-stamdata)", () => {
    expect(MUTATING_COMMANDS.has("company sync-cvr")).toBe(true);
  });

  // Audit 2026-06-11 (AGENT-3): `company set-profile` skriver navn, CVR,
  // adresse, payment_terms_days, bank og VAT-periode til company-db'en via
  // setCompanyProfile + setCompanyVatPeriodType — samme bug-klasse som
  // `company sync-cvr`. Den SKAL være actor-gated.
  test("company set-profile er en actor-gated mutation (skriver profil-stamdata)", () => {
    expect(MUTATING_COMMANDS.has("company set-profile")).toBe(true);
  });

  // Audit 2026-06-11 (AGENT-3): `import contacts` lander en Dinero-kontakt-CSV
  // i customer/vendor-master-data via createCustomer/createVendor (begge
  // insertAuditLog-attribuerede skrivninger). De enkeltvise `customer create`/
  // `vendor create` ER gated, så bulk-import-stien SKAL også være det.
  test("import contacts er en actor-gated mutation (skriver master-data)", () => {
    expect(MUTATING_COMMANDS.has("import contacts")).toBe(true);
  });

  // Audit 2026-06-11 (AGENT-3): `system export-saft` skriver en
  // `saft_export`-række til audit_log (insertAuditLog) — præcis som de allerede
  // gated `system export-authority`/`export-accountant`. Den SKAL gates ens.
  test("system export-saft er en actor-gated mutation (skriver audit_log-event)", () => {
    expect(MUTATING_COMMANDS.has("system export-saft")).toBe(true);
  });

  test("efaktura registrer-test-gln er actor-gated", () => {
    expect(MUTATING_COMMANDS.has("efaktura registrer-test-gln")).toBe(true);
  });

  test("efaktura registrer-test-afsender er actor-gated", () => {
    expect(MUTATING_COMMANDS.has("efaktura registrer-test-afsender")).toBe(true);
  });

  // Audit 2026-06-11 (AGENT-3): COMPLETENESS-værn. En kurateret liste over de
  // CLI-kommandoer hvis handler skriver til company-db'en eller audit_log'en
  // (verificeret ved at læse src/cli/*.ts-handlerne mod deres core-funktioner).
  // Hver SKAL stå i MUTATING_COMMANDS, ellers kører den uden actor-gate og
  // skriver en u-attribueret række. Tilføj nye db/audit-skrivende kommandoer
  // her samtidig med at de tilføjes i MUTATING_COMMANDS — det fanger den
  // klasse af fund auditten påviste (sync-cvr, set-profile, import contacts,
  // export-saft) før de når produktion.
  //
  // BEVIDST UDELADT (ikke db/audit-skrivende ELLER onboarding-bootstrap):
  //   - `company add` / `init`: onboarding-bootstrap der SELV seeder
  //     actor_allowlist (createCompany onboardingActor). Allowlisten findes
  //     ikke før den kører, så en gate ville være cirkulær — samme klasse som
  //     `init`. Workspace-niveau, ikke en ledger-skrivning.
  //   - `system rotate-backup-keypair`: skriver KUN nøglefiler til disk
  //     (mode 0o600), ingen db/audit_log-række. Nøgle-administration, ikke en
  //     attribueret ledger/audit-skrivning.
  //   - `gdpr audit-log`: read-only ledger-læsning der evt. dumper en JSON-fil
  //     til --out. `gdpr export`/`discover` skriver derimod audit-events og er
  //     derfor medtaget nedenfor sammen med slettevejen.
  //   - `import archive`/`systems`: read-only.
  const DB_OR_AUDIT_WRITING_COMMANDS: string[] = [
    "accounts role-confirm",
    "company sync-cvr",
    "company set-profile",
    "import run",
    "import contacts",
    "system export-authority",
    "system export-accountant",
    "system export-saft",
    "gdpr discover",
    "gdpr export",
    "gdpr erase",
    "gdpr forget",
    "company-knowledge propose",
    "company-knowledge review",
    "company-knowledge supersede",
    "ownership propose",
    "ownership review",
    "ownership apply",
  ];

  test("alle db/audit-skrivende kommandoer er actor-gated (completeness)", () => {
    for (const key of DB_OR_AUDIT_WRITING_COMMANDS) {
      expect(
        MUTATING_COMMANDS.has(key),
        `'${key}' skriver til db/audit_log men mangler i MUTATING_COMMANDS — ` +
          "den ville køre uden actor-gate og skrive en u-attribueret række",
      ).toBe(true);
    }
  });

  test("alle MUTATING_COMMANDS-nøgler svarer til registrerede command specs", () => {
    // Værn mod tastefejl: en nøgle i MUTATING_COMMANDS uden et spec gates
    // aldrig i praksis (cli.ts slår op på den dispatch-byggede commandKey).
    const specKeys = new Set(COMMAND_SPECS.map((spec) => spec.key));
    for (const key of MUTATING_COMMANDS) {
      expect(specKeys.has(key), `MUTATING_COMMANDS-nøglen '${key}' har intet command spec`).toBe(
        true,
      );
    }
  });

  test("workspace-only mutations never require an undocumented company", () => {
    const workspaceMutations = COMMAND_SPECS.filter(spec =>
      MUTATING_COMMANDS.has(spec.key) && spec.allowedFlags.includes("--workspace") && !spec.allowedFlags.includes("--company"),
    );
    expect(workspaceMutations.length).toBeGreaterThan(0);
    for (const spec of workspaceMutations) {
      expect(mutationPolicyScope(spec.key), `${spec.key}: ${spec.usage}`).not.toBe("company");
    }
    expect(mutationPolicyScope("system restore-backup")).toBe("restore-target");
    for (const subcommand of ["bootstrap-first", "bootstrap-local-service", "local-service-rotate", "local-service-revoke"]) {
      expect(mutationPolicyScope(`workspace-access ${subcommand}`)).toBe("workspace-bootstrap");
    }
    expect(mutationPolicyScope("group apply-manifest")).toBe("group-policy");
    expect(mutationPolicyScope("period reopen")).toBe("company");
  });
});

describe("global hjælp grupperer efter governance-klasse", () => {
  const usage = renderGlobalUsage();
  const readHeadingIdx = usage.indexOf("Læsekommandoer");
  const writeHeadingIdx = usage.indexOf("Skrivekommandoer");

  function sectionOf(commandKey: string): "read" | "write" | "other" {
    // Kommandonøglen står på sin egen linje med padding — match linjestart.
    const lineMatch = usage
      .split("\n")
      .findIndex((line) => line.startsWith(`  ${commandKey} `) || line.trim() === commandKey);
    if (lineMatch === -1) return "other";
    const offset = usage.split("\n").slice(0, lineMatch).join("\n").length;
    if (offset > writeHeadingIdx) return "write";
    if (offset > readHeadingIdx) return "read";
    return "other";
  }

  test("hjælpen har begge overskrifter", () => {
    expect(readHeadingIdx).toBeGreaterThan(-1);
    expect(writeHeadingIdx).toBeGreaterThan(readHeadingIdx);
  });

  test("gdpr forget vises under Skrivekommandoer, ikke under Læsekommandoer", () => {
    expect(sectionOf("gdpr forget")).toBe("write");
  });

  test("gdpr discover og export vises som writes, mens audit-log forbliver read-only", () => {
    expect(sectionOf("gdpr discover")).toBe("write");
    expect(sectionOf("gdpr export")).toBe("write");
    expect(sectionOf("gdpr audit-log")).toBe("read");
  });

  test("company sync-cvr vises under Skrivekommandoer", () => {
    expect(sectionOf("company sync-cvr")).toBe("write");
  });
});

describe("actor-gaten håndhæves for de to nyklassificerede kommandoer", () => {
  /**
   * Kører enforceMutationActorPolicy med ALLE actor-kilder fjernet (ingen
   * --actor, ingen env). For en mutation skal det fejle med
   * "actor required for mutations" — præcis den adfærd auditten påviste
   * manglede for `gdpr forget`.
   */
  function runGateWithoutActor(commandKey: string): string | null {
    const saved: Record<string, string | undefined> = {};
    const vars = [
      "RENTEMESTER_ACTOR",
      "RENTEMESTER_ACTOR_VIA",
      "OPENCLAW_AGENT",
      "RENTEMESTER_AGENT",
      "RENTEMESTER_USER",
      "USER",
      "LOGNAME",
      "USERNAME",
    ];
    for (const v of vars) {
      saved[v] = process.env[v];
      delete process.env[v];
    }
    const root = mkdtempSync(join(tmpdir(), "rentemester-gov-gate-"));
    try {
      let fatalMessage: string | null = null;
      enforceMutationActorPolicy(commandKey, root, null, null, (message: string) => {
        fatalMessage = message;
        throw new Error(`fatal: ${message}`);
      });
      return fatalMessage;
    } catch (err) {
      return err instanceof Error ? err.message.replace(/^fatal: /, "") : String(err);
    } finally {
      rmSync(root, { recursive: true, force: true });
      for (const v of vars) {
        if (saved[v] === undefined) delete process.env[v];
        else process.env[v] = saved[v];
      }
    }
  }

  test("gdpr forget uden actor afvises (alias og kanonisk navn opfører sig ens)", () => {
    expect(runGateWithoutActor("gdpr erase")).toContain("actor required for mutations");
    expect(runGateWithoutActor("gdpr forget")).toContain("actor required for mutations");
  });

  test("gdpr discover og export uden actor afvises, fordi de skriver audit-events", () => {
    expect(runGateWithoutActor("gdpr discover")).toContain("actor required for mutations");
    expect(runGateWithoutActor("gdpr export")).toContain("actor required for mutations");
  });

  test("company sync-cvr uden actor afvises", () => {
    expect(runGateWithoutActor("company sync-cvr")).toContain("actor required for mutations");
  });
});
