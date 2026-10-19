import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

/** Call after checkNativeAnnualReport, initially and after restart; real authenticated HTTP only. */
export async function checkNativeAnnualClients(page: Page, base: string, restarted = false) {
  const saved = JSON.parse(readFileSync("/tmp/rentemester-rust-reports/native-annual-report.json", "utf8"));
  const { fiscalYearStart, fiscalYearEnd } = saved;
  if (typeof fiscalYearStart !== "string" || typeof fiscalYearEnd !== "string") throw new Error("Annual preparation evidence missing explicit fiscal bounds");
  const response = await page.request.get(`${base}/api/companies/synthetic/annual-report?${new URLSearchParams({ fiscalYearStart, fiscalYearEnd })}`);
  if (!response.ok()) throw new Error(`Actual annual HTTP read refused: ${response.status()}`);
  const expected = await response.json();
  const temp = mkdtempSync("/tmp/rm-native-annual-clients-");
  try {
    const cookieFile = join(temp, "session.cookie");
    writeFileSync(cookieFile, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookieFile];
    const cli = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", "annual-report", ...common, "--company", "synthetic", "--fiscal-year-start", fiscalYearStart, "--fiscal-year-end", fiscalYearEnd], { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
    if (cli.error || cli.status !== 0) throw new Error("Actual CLI annual-report failed; build rentemester-cli or configure RM_NATIVE_CLI_BIN");
    const input = [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-annual-check", version: "1" } } },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "annual_report", arguments: { company: "synthetic", fiscalYearStart, fiscalYearEnd } } },
    ].map(v => JSON.stringify(v)).join("\n") + "\n";
    const mcp = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
    if (mcp.error || mcp.status !== 0) throw new Error("Actual MCP annual_report failed; build rentemester-mcp or configure RM_NATIVE_MCP_BIN");
    const call = mcp.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
    if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error("MCP annual_report returned protocol/tool error");
    function normalized(value: unknown): string {
      function sorted(v: unknown): unknown {
        if (Array.isArray(v)) return v.map(sorted);
        if (v !== null && typeof v === "object") return Object.fromEntries(Object.entries(v).sort(([a], [b]) => a.localeCompare(b)).map(([key, item]) => [key, sorted(item)]));
        return v;
      }
      return JSON.stringify(sorted(value));
    }
    if (normalized(expected.annualReport) !== normalized(saved)) throw new Error("Actual HTTP annual preparation differs from persisted browser evidence");
    for (const [name, value] of [["CLI", JSON.parse(cli.stdout).data], ["MCP", call.result.structuredContent.data]] as const) {
      if (normalized(value) !== normalized(expected)) throw new Error(`${name} annual preparation differs from full actual HTTP JSON`);
    }
    console.log(restarted ? "PASS: CLI/MCP full annual preparation equals authenticated HTTP and persisted evidence after restart" : "PASS: CLI/MCP full annual preparation equals authenticated HTTP and saved browser period evidence");
  } finally { rmSync(temp, { recursive: true, force: true }); }
}
