// Tests: src/cli/import.ts, src/cli-args.ts — the `import contacts` CLI
// command, including the --enrich-cvr boolean flag and CVR enrichment wiring.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
const CSV =
  "Kontaktnavn;Adresse;Postnummer;By;Landekode;CVR-nummer;EAN-nummer;Telefon;E-mail;Att. person;Hjemmeside;Betalings metode;Betalingsfrist i dage;Total salg;Total køb;Kontakttype\n" +
  "CLI Test ApS;Vej 1;8000;Aarhus C;DK;12345678;;;;;;Netto;8;0;1000;Company\n";

describe("import contacts CLI", () => {
  test("plain import lands contacts without touching the network", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-imp-cli-"));
    const company = join(root, "company");
    const csvPath = join(root, "Kontakter.csv");
    writeFileSync(csvPath, CSV);

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "import", "contacts", "--company", company, "--file", csvPath],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect(exitCode).toBe(0);
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.summary.vendorsCreated).toBe(1);
  });

  test("--enrich-cvr is a value-less flag and enriches DK contacts", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-imp-cli-cvr-"));
    const company = join(root, "company");
    const csvPath = join(root, "Kontakter.csv");
    writeFileSync(csvPath, CSV);

    // A stub CVR Elasticsearch endpoint returning one company with an email.
    const server = Bun.serve({
      port: 0,
      fetch() {
        return Response.json({
          hits: {
            total: 1,
            hits: [
              {
                _source: {
                  Vrvirksomhed: {
                    cvrNummer: 12345678,
                    navne: [{ navn: "CLI Test ApS", periode: { gyldigTil: null } }],
                    elektroniskPost: [
                      { kontaktoplysning: "cli@test.dk", hemmelig: false, periode: { gyldigTil: null } },
                    ],
                    virksomhedMetadata: { nyesteNavn: { navn: "CLI Test ApS" }, sammensatStatus: "NORMAL" },
                  },
                },
              },
            ],
          },
        });
      },
    });
    const env = {
      ...process.env,
      RENTEMESTER_CVR_ENDPOINT: `http://127.0.0.1:${server.port}`,
      CVR_USERNAME: "test",
      CVR_PASSWORD: "test",
    };

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "import", "contacts", "--company", company, "--file", csvPath, "--enrich-cvr"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe", env },
    );
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    server.stop(true);
    cleanupDir(root);

    // The flag must parse as a boolean — not error "requires a value".
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.summary.enriched).toBe(1);
  });
});
