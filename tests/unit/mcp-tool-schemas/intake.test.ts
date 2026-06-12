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

describe("#274 — intake tools expose the full DocumentMetadata schema", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  function metadataSchemaOf(name: string) {
    const tool = tools.find((t) => t.name === name);
    expect(tool, `tool ${name} not found`).toBeDefined();
    return tool.inputSchema?.properties?.metadata as any;
  }

  // The named DocumentMetadata properties an external agent must be able to see.
  const EXPECTED_METADATA_PROPS = [
    "documentType",
    "issueDate",
    "invoiceNo",
    "deliveryDescription",
    "amountIncVat",
    "currency",
    "sender",
    "recipient",
    "vatAmount",
    "paymentDetails",
    "exemptionCode",
  ];

  for (const toolName of ["imap_intake_poll", "mail_intake_ingest"]) {
    test(`${toolName}.metadata is the unfolded DocumentMetadata, not an opaque object`, () => {
      const metadata = metadataSchemaOf(toolName);
      expect(metadata?.type, `${toolName}.metadata.type`).toBe("object");
      const props = metadata?.properties ?? {};
      // The old bare `type: object` catchall has zero named properties.
      expect(
        Object.keys(props).length,
        `${toolName}.metadata named properties`,
      ).toBeGreaterThanOrEqual(EXPECTED_METADATA_PROPS.length);
      for (const prop of EXPECTED_METADATA_PROPS) {
        expect(props[prop], `${toolName}.metadata.${prop}`).toBeDefined();
        expect(
          typeof props[prop]?.description,
          `${toolName}.metadata.${prop} description`,
        ).toBe("string");
      }
    });
  }

  test("the intake metadata schema matches documents_ingest.metadata", () => {
    // Reused definition: imap_intake_poll / mail_intake_ingest must carry the
    // SAME named properties as documents_ingest so they cannot drift apart.
    const ingestProps = Object.keys(
      metadataSchemaOf("documents_ingest")?.properties ?? {},
    ).sort();
    for (const toolName of ["imap_intake_poll", "mail_intake_ingest"]) {
      const props = Object.keys(metadataSchemaOf(toolName)?.properties ?? {}).sort();
      // The intake variants omit `source` (it is set by the pipeline), so they
      // must equal documents_ingest's properties minus `source`.
      const expected = ingestProps.filter((p) => p !== "source");
      expect(props, `${toolName}.metadata properties`).toEqual(expected);
    }
  });
});
