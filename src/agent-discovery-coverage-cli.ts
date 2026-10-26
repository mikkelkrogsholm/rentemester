#!/usr/bin/env bun
/** Deterministic #585 gate over the actual runtime registries. */
import { MUTATING_COMMANDS } from "./cli-actor";
import { COMMAND_SPECS, SIDE_EFFECTING_COMMANDS } from "./cli-meta";
import { registerAllTools } from "./mcp/registry";
import { operationIdentityForLiveTool } from "./mcp/operation-registry";
import { ROUTE_CATALOG } from "./server/router";
import {
  validateAgentDiscoveryCoverage,
  type LiveTool,
} from "./agent-discovery-catalog";

function toolsForProfile(profile: "compact" | "full"): LiveTool[] {
  const recorder = { registerTool() {} };
  const registry = registerAllTools(recorder as never, undefined, { profile });
  return registry.operations.map((record) => operationIdentityForLiveTool(record, profile === "full"));
}

function coverageForProfile(profile: "compact" | "full") {
  return validateAgentDiscoveryCoverage({
    tools: toolsForProfile(profile),
    commands: COMMAND_SPECS.map((command) => ({
      key: command.key,
      allowedFlags: command.allowedFlags,
      mutating: MUTATING_COMMANDS.has(command.key),
      sideEffecting: SIDE_EFFECTING_COMMANDS.has(command.key),
    })),
    routes: ROUTE_CATALOG,
    imageDigest: process.env.RENTEMESTER_AGENT_DISCOVERY_IMAGE_DIGEST ?? null,
  });
}

const compactReport = coverageForProfile("compact");
const fullReport = coverageForProfile("full");
const report = {
  ...compactReport,
  ok: compactReport.ok && fullReport.ok,
  errors: [
    ...compactReport.errors,
    ...fullReport.errors.map((error) => `full profile: ${error}`),
  ],
  profiles: {
    compact: { ok: compactReport.ok, counts: compactReport.counts, coverageHash: compactReport.coverageHash },
    full: { ok: fullReport.ok, counts: fullReport.counts, coverageHash: fullReport.coverageHash },
  },
};
process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
if (!report.ok) {
  for (const error of report.errors) console.error(`agent-discovery coverage: ${error}`);
  process.exit(1);
}
