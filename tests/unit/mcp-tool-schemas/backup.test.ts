// Tests: src/mcp/tool-runtime.ts, src/mcp/tools (typed payload schemas + confirm envelope)
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { startMcpFixture, stopMcpFixture, type StdioMcpClient } from "./_shared";

let companyRoot: string;
let client: StdioMcpClient;

beforeAll(async () => {
  ({ companyRoot, client } = await startMcpFixture());
});

afterAll(async () => {
  await stopMcpFixture(companyRoot, client);
});

describe("#275 — system_backup_destination_add documents kind + attestations", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  function schemaOf(name: string) {
    const tool = tools.find((t) => t.name === name);
    expect(tool, `tool ${name} not found`).toBeDefined();
    return tool.inputSchema as any;
  }

  test("kind is an enum exposing the five valid destination kinds", () => {
    const kind = schemaOf("system_backup_destination_add").properties?.kind;
    expect(Array.isArray(kind?.enum), "kind must be an enum").toBe(true);
    for (const value of ["local-folder", "dropbox", "google-drive", "ssh", "other"]) {
      expect(kind?.enum, `kind enum missing ${value}`).toContain(value);
    }
    expect(typeof kind?.description).toBe("string");
  });

  test("every attestation field carries a description", () => {
    const props = schemaOf("system_backup_destination_add").properties ?? {};
    for (const field of [
      "inEeaOrEu",
      "nonRelatedParty",
      "itSecurityMeetsStandards",
      "regionCountry",
      "attestedBy",
    ]) {
      expect(typeof props[field]?.description, `${field} description`).toBe("string");
      expect((props[field]?.description ?? "").length, `${field} description length`).toBeGreaterThan(
        10,
      );
    }
  });

  test("inEeaOrEu and attestedBy descriptions carry the human-attestation warning", () => {
    const props = schemaOf("system_backup_destination_add").properties ?? {};
    const inEea: string = (props.inEeaOrEu?.description ?? "").toLowerCase();
    expect(inEea).toContain("attest");
    // The legal anchor must be present.
    expect(inEea).toContain("205/2024");
    const attestedBy: string = (props.attestedBy?.description ?? "").toLowerCase();
    // It must be clear a human attests — Rentemester cannot know it itself.
    expect(attestedBy).toContain("human");
  });
});
