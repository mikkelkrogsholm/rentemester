#!/usr/bin/env bun
import { verifyEvidence } from "./cockpit-evidence";

const path = process.argv[2];
if (!path) throw new Error("usage: verify-cockpit-evidence.ts <cockpit-evidence.json>");
const manifest = verifyEvidence(path);
const regressionsPath = process.argv[3];
if (regressionsPath) {
  const issues = JSON.parse(await Bun.file(regressionsPath).text()) as Array<{ number?: number; title?: string; labels?: Array<{ name?: string }> }>;
  const blocking = issues.filter((issue) => issue.labels?.some((label) => label.name === "severity:critical" || label.name === "severity:high"));
  if (blocking.length) throw new Error(`open related severity:critical or severity:high regression blocks Cockpit evidence: ${blocking.map((issue) => `#${issue.number} ${issue.title ?? ""}`.trim()).join(", ")}`);
}
process.stdout.write(`verified ${manifest.scenarios.length} Cockpit screenshots for ${manifest.image}\n`);
