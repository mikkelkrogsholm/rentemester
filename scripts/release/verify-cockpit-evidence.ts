#!/usr/bin/env bun
import { verifyEvidence } from "./cockpit-evidence";

const path = process.argv[2];
if (!path)
  throw new Error("usage: verify-cockpit-evidence.ts <cockpit-evidence.json>");
const manifest = verifyEvidence(path);
process.stdout.write(
  `verified ${manifest.scenarios.length} Cockpit screenshots for ${manifest.image}\n`,
);
