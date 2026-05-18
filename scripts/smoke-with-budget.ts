#!/usr/bin/env bun
// Runs `bun run smoke` and fails if wall-clock time exceeds BUDGET_SECONDS.
// Override via SMOKE_BUDGET_SECONDS env-var. Default budget: 30 seconds.
import { spawn } from "bun";

const BUDGET_SECONDS = Number(process.env.SMOKE_BUDGET_SECONDS ?? 30);
const start = Date.now();
const proc = spawn(["bun", "run", "smoke"], {
  stdout: "inherit",
  stderr: "inherit",
});
const exitCode = await proc.exited;
const elapsed = (Date.now() - start) / 1000;
console.log(
  `\nsmoke wall-clock: ${elapsed.toFixed(2)}s (budget ${BUDGET_SECONDS}s)`,
);
if (exitCode !== 0) {
  process.exit(exitCode);
}
if (elapsed > BUDGET_SECONDS) {
  console.error(
    `smoke exceeded budget: ${elapsed.toFixed(2)}s > ${BUDGET_SECONDS}s`,
  );
  process.exit(1);
}
