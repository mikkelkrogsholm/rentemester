// Tests: confirm-konventionen er dokumenteret konsistent på tværs af
// docs/confirm-contract.md, docs/mcp-agent-contract.md, docs/cockpit-api.md
// og docs/cli-contract.md (#369).
//
// Hvorfor denne guard findes: confirm-konventionen er uensartet pr. design
// (cockpit har en modal som samtykke, CLI har en append-only BOOLEAN_FLAGS-
// liste der tvinger valued `--confirm yes`, MCP har intet UI og kræver det
// derfor på alle writes). En agent skal kunne slå dette op ét sted —
// `docs/confirm-contract.md` — og de tre stak-docs SKAL linke dertil. Hvis
// nogen flytter eller omdøber siden, fanger denne test det.
import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const REPO_ROOT = new URL("../..", import.meta.url).pathname.replace(/\/$/, "");

function readDoc(rel: string): string {
  const path = join(REPO_ROOT, rel);
  expect(existsSync(path), `${rel} skal eksistere`).toBe(true);
  return readFileSync(path, "utf8");
}

describe("Confirm-konvention dokumenteret konsistent (#369)", () => {
  test("docs/confirm-contract.md eksisterer og har de obligatoriske afsnit", () => {
    const text = readDoc("docs/confirm-contract.md");
    // Hovedstruktur — én tabel pr. business-operation + sprog-/match-tabel
    expect(text).toContain("Confirm-konventionen");
    expect(text).toContain("Princippet");
    expect(text).toContain("Tabellen");
    expect(text).toContain("string-match");
    // De tre stak-overskrifter skal optræde i opslaget
    expect(text).toContain("MCP");
    expect(text).toContain("Cockpit");
    expect(text).toContain("CLI");
    // Den eksakte cockpit-fejl skal stå dér så string-matchende agenter kan slå op
    expect(text).toContain("denne handling er irreversibel og kræver 'confirm: true'");
    // Den fælles MCP-prefix der fanger både write- og destructive-tools
    expect(text).toContain("confirm: true required for ");
    // Den dokumenterede MCP-vs-cockpit afvigelse skal være eksplicit
    expect(text).toContain("invoice_issue");
    expect(text).toContain("POST /invoices/issue");
    // CLI-syntaxen
    expect(text).toContain("--confirm yes");
  });

  test("docs/cli-contract.md har et 'Confirm-flag'-afsnit der linker til opslaget", () => {
    const text = readDoc("docs/cli-contract.md");
    expect(text).toContain("## 3. Confirm-flag");
    expect(text).toContain("--confirm yes");
    // Skal linke til opslaget
    expect(text).toMatch(/confirm-contract\.md/);
    // Skal nævne de to kommandoer der bruger flaget
    expect(text).toContain("system restore-backup");
    expect(text).toContain("asset write-off");
  });

  test("docs/mcp-agent-contract.md linker til confirm-contract.md fra confirm-afsnittet", () => {
    const text = readDoc("docs/mcp-agent-contract.md");
    expect(text).toContain("### The confirm convention");
    expect(text).toMatch(/confirm-contract\.md/);
    // Den fælles match-prefix skal stå (var allerede der; vi guard'er at den ikke fjernes)
    expect(text).toContain("confirm: true required for");
  });

  test("docs/cockpit-api.md linker til confirm-contract.md fra confirm gate-afsnittet", () => {
    const text = readDoc("docs/cockpit-api.md");
    expect(text).toContain("The confirm gate");
    expect(text).toMatch(/confirm-contract\.md/);
    // Den eksakte danske fejl-tekst skal være uændret
    expect(text).toContain("denne handling er irreversibel og kræver 'confirm: true'");
  });

  test("Cockpittets afvisningsbesked er identisk i koden og i begge docs", () => {
    // Implementeringen er sandheden — alt skal matche den
    const mutations = readFileSync(
      join(REPO_ROOT, "src/server/mutations.ts"),
      "utf8",
    );
    const exactMessage = "denne handling er irreversibel og kræver 'confirm: true'";
    expect(mutations).toContain(exactMessage);
    expect(readDoc("docs/cockpit-api.md")).toContain(exactMessage);
    expect(readDoc("docs/confirm-contract.md")).toContain(exactMessage);
  });

  test("MCP's write-tool-fejlmeddelelse er identisk i koden og i begge docs", () => {
    const runtime = readFileSync(
      join(REPO_ROOT, "src/mcp/tool-runtime.ts"),
      "utf8",
    );
    // Substring-formen (uden tool-navnet) er kontrakten
    const writePrefix = "confirm: true required for write tool ";
    const destructivePrefix = "confirm: true required for destructive tool ";
    expect(runtime).toContain(writePrefix);
    // Destructive-prefix står i den dedikerede helper — vi tjekker bare at
    // den fælles prefix er dokumenteret som match-strategi i opslaget
    expect(readDoc("docs/confirm-contract.md")).toContain("confirm: true required for ");
    expect(readDoc("docs/mcp-agent-contract.md")).toContain(writePrefix);
    expect(readDoc("docs/mcp-agent-contract.md")).toContain(destructivePrefix);
  });

  test("CLI's '--confirm yes'-afvisning er identisk i koden og i opslaget", () => {
    const systemCli = readFileSync(
      join(REPO_ROOT, "src/cli/system.ts"),
      "utf8",
    );
    const exactSuffix = "Re-run with --confirm yes to proceed.";
    expect(systemCli).toContain(exactSuffix);
    expect(readDoc("docs/confirm-contract.md")).toContain(exactSuffix);
  });
});
