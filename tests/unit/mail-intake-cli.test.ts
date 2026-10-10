// Tests: src/cli/mail-intake.ts, src/cli.ts (bilagsmail intake CLI — #122)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

function buildEml(messageId: string, opts: { noAttachment?: boolean } = {}): string {
  const headers = [
    "From: Leverandør ApS <faktura@leverandor.dk>",
    "Subject: Faktura",
    "Date: Mon, 16 May 2026 09:00:00 +0000",
    `Message-ID: ${messageId}`,
    "MIME-Version: 1.0",
  ];
  if (opts.noAttachment) {
    return [...headers, "Content-Type: text/plain; charset=utf-8", "", "Ingen vedhæftning.", ""].join("\r\n");
  }
  const b64 = Buffer.from("%PDF-1.4\n%minimal pdf body\n").toString("base64");
  return [
    ...headers,
    'Content-Type: multipart/mixed; boundary="rmb"',
    "",
    "--rmb",
    "Content-Type: text/plain; charset=utf-8",
    "",
    "Se bilag.",
    "--rmb",
    'Content-Type: application/pdf; name="faktura.pdf"',
    "Content-Transfer-Encoding: base64",
    'Content-Disposition: attachment; filename="faktura.pdf"',
    "",
    b64,
    "--rmb--",
    "",
  ].join("\r\n");
}

const metadata = {
  issueDate: "2026-05-16",
  invoiceNo: "INV-CLI-1",
  deliveryDescription: "Bogføring og momsafstemning",
  amountIncVat: 1250,
  currency: "DKK",
  sender: { name: "Leverandør ApS", address: "Sælgervej 1, 2100 København Ø", vatOrCvr: "DK11223344" },
  recipient: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
  vatAmount: 250,
};

describe("mail-intake ingest CLI", () => {
  test("ingests an EML file and reports a stable result across reruns", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-mailcli-"));
    const company = join(root, "company");
    const eml = join(root, "message.eml");
    const metaFile = join(root, "metadata.json");
    writeFileSync(eml, buildEml("<cli-1@example.com>"));
    writeFileSync(metaFile, JSON.stringify(metadata, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const run = (label: string) =>
      Bun.spawn(
        ["bun", "run", "src/cli.ts", "mail-intake", "ingest", "--company", company, "--source", eml, "--metadata", metaFile],
        { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
      );

    const firstProc = run("first");
    const firstStdout = await new Response(firstProc.stdout).text();
    const firstStderr = await new Response(firstProc.stderr).text();
    const firstExit = await firstProc.exited;
    expect({ exitCode: firstExit, stderr: firstStderr }).toEqual({ exitCode: 0, stderr: "" });
    const firstParsed = JSON.parse(firstStdout);
    expect(firstParsed.ok).toBe(true);
    expect(firstParsed.attachmentsIngested).toBe(1);

    const secondProc = run("second");
    const secondStdout = await new Response(secondProc.stdout).text();
    const secondExit = await secondProc.exited;
    expect(secondExit).toBe(0);
    const secondParsed = JSON.parse(secondStdout);
    expect(secondParsed.attachmentsIngested).toBe(0);
    expect(secondParsed.attachmentsSkipped).toBe(1);

    rmSync(root, { recursive: true, force: true });
  });

  test("ingests a no-attachment message as EML evidence with metadata (#566)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-mailcli-noatt-"));
    const company = join(root, "company");
    const eml = join(root, "no-attachment.eml");
    const metaFile = join(root, "metadata.json");
    writeFileSync(eml, buildEml("<cli-noatt@example.com>", { noAttachment: true }));
    writeFileSync(metaFile, JSON.stringify(metadata, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "mail-intake", "ingest", "--company", company, "--source", eml, "--metadata", metaFile],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.evidenceIngested).toBe(1);
    expect(parsed.exceptionsCreated).toBe(0);

    const docProc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "documents", "list", "--company", company, "--format", "json"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const docStdout = await new Response(docProc.stdout).text();
    await docProc.exited;
    expect(docStdout).toContain(".eml");
    expect(docStdout).toContain("mail-intake:<cli-noatt@example.com>");

    rmSync(root, { recursive: true, force: true });
  });
});
