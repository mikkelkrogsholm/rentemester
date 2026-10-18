// Real browser journey against run-web-demo.py --check; no intercepted requests or fixtures.
import { checkNativeSettlement } from "./check-native-settlement";
import { checkNativeSales } from "./check-native-sales";
import { checkNativeVat } from "./check-native-vat";
import { checkNativeCompanySetup } from "./check-native-company-setup";
import { checkNativeFxExpense } from "./check-native-fx-expense";
import { checkNativeExpense } from "./check-native-expense";
import { checkNativeDocuments } from "./check-native-documents";
import { checkNativeReports } from "./check-native-reports";
import { checkNativeReversal } from "./check-native-reversal";
import { chromium } from "@playwright/test";
import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { createHmac } from "node:crypto";

const base = process.env.RM_NATIVE_WEB_URL;
let verification = process.env.RM_NATIVE_WEB_LINK;
const setup = process.env.RM_NATIVE_WEB_SETUP;
if (!base || (!verification && !setup && !process.env.RM_NATIVE_WEB_REOPEN)) throw new Error("Start with: python3 scripts/rust/run-web-demo.py --check");
function totp(secret: string): string {
  let bits = 0, count = 0;
  const bytes: number[] = [];
  for (const char of secret) {
    const value = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".indexOf(char);
    if (value < 0) throw new Error("Invalid synthetic TOTP setup");
    bits = (bits << 5) | value; count += 5;
    if (count >= 8) { count -= 8; bytes.push((bits >>> count) & 255); }
  }
  const counter = Buffer.alloc(8); counter.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 30_000)));
  const digest = createHmac("sha1", Buffer.from(bytes)).update(counter).digest();
  const offset = digest[19] & 15;
  return String((digest.readUInt32BE(offset) & 0x7fff_ffff) % 1_000_000).padStart(6, "0");
}
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(30_000);
  page.setDefaultNavigationTimeout(60_000);
  const failures: string[] = [];
  page.on("pageerror", (error) => failures.push(error.message));
  let postedBody: string | null = null;
  page.on("request", (request) => { if (request.url().endsWith("/manual-journal") && request.method() === "POST") postedBody = request.postData(); });
  const statePath = process.env.RM_NATIVE_WEB_STATE;
  if (process.env.RM_NATIVE_WEB_REOPEN) {
    if (!statePath) throw new Error("Missing synthetic reopen state");
    const state = JSON.parse(readFileSync(statePath, "utf8"));
    await page.goto(base);
    await page.getByLabel("E-mail", { exact: true }).fill("owner@example.invalid");
    await page.getByLabel("Adgangskode", { exact: true }).fill("synthetic-password-123");
    await page.getByRole("button", { name: "Log ind", exact: true }).click();
    await page.getByRole("button", { name: "Brug recovery code" }).click();
    await page.getByLabel("Recovery code", { exact: true }).fill(process.env.RM_NATIVE_WEB_EXPENSE_REOPEN === "1" ? state.backup2 : state.backup);
    await page.getByRole("button", { name: "Bekræft", exact: true }).click();
    await page.getByRole("link", { name: "Åbn bogføring" }).click();
    await page.getByText("Synthetic opening transfer", { exact: true }).waitFor();
    if (process.env.RM_NATIVE_WEB_EXPENSE_REOPEN === "1") { await checkNativeCompanySetup(page, base, true); await checkNativeVat(page, base, true); await checkNativeSales(page, base, true); await checkNativeSettlement(page, base, true); } else {
    // Replaying the same browser operation must not create another journal entry.
    const replay = await page.request.post(`${base}/api/companies/synthetic/manual-journal`, { headers: { origin: base, "content-type": "application/json" }, data: JSON.parse(state.postedBody) });
    if (!replay.ok()) throw new Error(`Idempotency replay refused: ${replay.status()}`);
    await page.reload();
    await page.getByText("Synthetic opening transfer", { exact: true }).waitFor();
    if (await page.getByText("Synthetic opening transfer", { exact: true }).count() !== 1) throw new Error("Duplicate posting after replay");
    await page.locator("section.statement").getByRole("link", { name: "Saldobalance", exact: true }).click();
    await page.getByText("Saldobalancen stemmer — debet = kredit.", { exact: true }).waitFor();
    const rows = await page.locator("section.statement tbody tr").allTextContents();
    if (rows.length !== 2 || !rows.every((row) => row.includes("125,50"))) throw new Error("Reopened trial balance changed");
    await page.screenshot({ path: process.env.RM_NATIVE_WEB_SCREENSHOT ?? "/tmp/rentemester-rust-first-journey.png", fullPage: true });
    console.log("PASS: Rust server restart → browser login → persisted posting and balance; idempotent replay creates no duplicate");
    await checkNativeDocuments(page, base, true);
    await checkNativeReversal(page, base);
    await checkNativeExpense(page, base);
    await checkNativeFxExpense(page, base);
    await checkNativeCompanySetup(page, base);
    await checkNativeVat(page, base);
    await checkNativeSales(page, base);
    await checkNativeSettlement(page, base);
    }
  } else {
  if (setup) {
    const unauthorized = await page.request.post(`${base}/api/setup/first-owner`, { headers: { origin: base, "content-type": "application/json", "x-rentemester-install-token": "invalid" }, data: { name: "Synthetic Intruder", email: "intruder@example.invalid", password: "synthetic-password-456" } });
    if (unauthorized.status() !== 403) throw new Error("Installer accepted an invalid capability");
    await page.goto(setup);
    await page.getByLabel("Dit navn").fill("Synthetic Owner");
    await page.getByLabel("E-mail", { exact: true }).fill("owner@example.invalid");
    await page.getByLabel("Ny adgangskode", { exact: true }).fill("synthetic-password-123");
    await page.getByLabel("Gentag adgangskode", { exact: true }).fill("synthetic-password-123");
    await page.getByRole("button", { name: "Opret ejer", exact: true }).click();
    await page.getByRole("heading", { name: "Første ejer er oprettet" }).waitFor();
    const container = process.env.RM_NATIVE_WEB_CONTAINER;
    if (!container || !/^rm-native-web-[a-f0-9]+$/.test(container)) throw new Error("Missing synthetic outbox container");
    verification = execFileSync("docker", ["exec", container, "sh", "-c", 'for f in /qualified/parent/workspace/.rentemester/verification-*.txt; do cat "$f"; break; done'], { encoding: "utf8" });
    await page.getByRole("button", { name: "Til login", exact: true }).click();
    const consumed = await page.request.post(`${base}/api/setup/first-owner`, { headers: { origin: base, "content-type": "application/json", "x-rentemester-install-token": new URLSearchParams(new URL(setup).hash.slice(1)).get("setup")! }, data: { name: "Synthetic Intruder", email: "intruder@example.invalid", password: "synthetic-password-456" } });
    if (consumed.status() !== 403) throw new Error("Consumed installer capability was reused");
    console.log("Browser: first owner created through private installer; invalid and consumed capabilities refused");
  }
  await page.goto(verification!);
  await page.goto(base);
  console.log("Browser: verification link accepted; opening login");
  await page.getByLabel("E-mail", { exact: true }).fill("owner@example.invalid");
  await page.getByLabel("Adgangskode", { exact: true }).fill("synthetic-password-123");
  await page.getByRole("button", { name: "Log ind", exact: true }).click();
  await page.getByRole("heading", { name: "Opsæt totrinsbekræftelse" }).waitFor();
  await page.getByLabel("Adgangskode", { exact: true }).fill("synthetic-password-123");
  await page.getByRole("button", { name: "Start opsætning" }).click();
  await page.getByLabel("Opsætningsnøgle").waitFor();
  const secret = await page.getByLabel("Opsætningsnøgle").inputValue();
  await page.getByLabel("Engangskode", { exact: true }).fill(totp(secret));
  await page.getByRole("button", { name: "Bekræft opsætning" }).click();
  const backup = await page.getByRole("region", { name: "Recovery codes" }).locator("li").first().textContent();
  const backup2 = await page.getByRole("region", { name: "Recovery codes" }).locator("li").nth(1).textContent();
  await page.getByRole("button", { name: "Jeg har gemt koderne" }).click();
  console.log("Browser: native MFA and session complete");
  await page.getByRole("link", { name: "Åbn bogføring" }).click();
  console.log("Browser: company selected and journal loaded");
  await page.getByRole("button", { name: "Ny postering", exact: true }).click();
  await page.getByLabel("Posteringstekst").fill("Synthetic opening transfer");
  await page.getByLabel("Debetkonto").selectOption("1800");
  await page.getByLabel("Kreditkonto").selectOption("8000");
  await page.getByLabel("Beløb i DKK").fill("125,50");
  await page.getByRole("button", { name: "Gennemse postering" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Bogfør postering", exact: true }).click();
  await page.getByText("Synthetic opening transfer", { exact: true }).waitFor();
  console.log("Browser: manual journal post visible");
  await page.locator("section.statement").getByRole("link", { name: "Saldobalance", exact: true }).click();
  await page.getByRole("heading", { name: "Saldobalance", exact: true }).waitFor();
  const report = page.locator("section.statement");
  await report.getByText("Saldobalancen stemmer — debet = kredit.", { exact: true }).waitFor();
  const rows = await report.locator("tbody tr").allTextContents();
  if (rows.length !== 2 || !rows.every((row) => row.includes("125,50"))) throw new Error("Unexpected trial balance rows");
  await page.reload();
  await page.getByText("Saldobalancen stemmer — debet = kredit.", { exact: true }).waitFor();
  await checkNativeReports(page, base);
  await checkNativeDocuments(page, base);
  if (failures.length) throw new Error(`Browser errors: ${failures.join("; ")}`);
  await page.screenshot({ path: process.env.RM_NATIVE_WEB_SCREENSHOT ?? "/tmp/rentemester-rust-first-journey.png", fullPage: true });
  if (statePath) writeFileSync(statePath, JSON.stringify({ backup, backup2, postedBody }), { mode: 0o600 });
  console.log("PASS: real browser → Rust: first owner, verification, MFA, company selection, manual post, correct persisted trial balance");
  }
} finally { await browser.close(); }
