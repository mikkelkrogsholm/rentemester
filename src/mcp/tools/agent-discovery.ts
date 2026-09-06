import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { MUTATING_COMMANDS } from "../../cli-actor";
import { COMMAND_SPECS, SIDE_EFFECTING_COMMANDS } from "../../cli-meta";
import { describeWorkflow, searchCapabilities, type LiveTool } from "../../agent-discovery-catalog";
import { ROUTE_CATALOG } from "../../server/router";
import { envelopeShape, envelopeToCallResult, errorEnvelope, successEnvelope } from "../envelope";

/** Registers only read-only discovery tools. Their operation facts come from
 * the already registered runtime tool objects, never from a duplicate list. */
export function registerAgentDiscoveryTools(server: McpServer, liveTools: () => readonly LiveTool[]): void {
  const sources = () => ({
    tools: liveTools(),
    commands: COMMAND_SPECS.map((command) => ({
      key: command.key,
      allowedFlags: command.allowedFlags,
      mutating: MUTATING_COMMANDS.has(command.key),
      sideEffecting: SIDE_EFFECTING_COMMANDS.has(command.key),
    })),
    routes: ROUTE_CATALOG,
  });
  server.registerTool("agent_capability_search", {
    title: "Search supported agent outcomes", description: "Read-only, paginated search of the versioned Rentemester outcome catalogue. Start with system_server_about.",
    inputSchema: { query: z.string().max(200).optional(), cursor: z.number().int().min(0).optional(), limit: z.number().int().min(1).max(50).optional() }, outputSchema: envelopeShape,
    annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
  }, ({ query, cursor, limit }) => envelopeToCallResult(successEnvelope(searchCapabilities(query, cursor ?? 0, limit ?? 10, sources()))));
  server.registerTool("agent_workflow_describe", {
    title: "Describe a supported agent workflow", description: "Read-only full workflow with live MCP operation resolution, annotations-derived safety and retry/read-back boundaries.",
    inputSchema: { id: z.string().min(1).max(100) }, outputSchema: envelopeShape,
    annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
  }, ({ id }) => {
    const description = describeWorkflow(id, sources());
    return envelopeToCallResult(description ? successEnvelope(description) : errorEnvelope(["UNKNOWN_WORKFLOW"]));
  });
}
