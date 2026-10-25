/** Bounded packaged recovery adapter: ordinary browser identity, then GET-only readback.
 * Never invoke check-native-web's financial receipt replay on a restored namespace.
 */
import { chromium, request } from "@playwright/test";
import { createHash, createHmac, generateKeyPairSync } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";

const work = process.env.RM_CONTAINER_RECOVERY_DIR!;
const phase = process.argv[2];
const hash = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");
const save = (name: string, value: unknown) => writeFileSync(join(work, name), JSON.stringify(value), { mode: 0o600, flag: "wx" });
function need(value: unknown, message: string): asserts value { if (!value) throw Error(message); }
if (phase === "keys") {
  // The independent trust anchor is established BEFORE export, never read from an archive.
  const keys = generateKeyPairSync("ed25519");
  writeFileSync(join(work, "signing.der"), keys.privateKey.export({ type: "pkcs8", format: "der" }), { mode: 0o600, flag: "wx" });
  const jwk = keys.publicKey.export({ format: "jwk" });
  need(jwk.x, "Independent Ed25519 public key missing");
  writeFileSync(join(work, "trusted.pub"), Buffer.from(jwk.x, "base64url"), { mode: 0o600, flag: "wx" });
} else {
  const base = process.env.RM_NATIVE_WEB_URL!;
  need(/^http:\/\/127\.0\.0\.1:\d+$/.test(base), "Explicit synthetic loopback origin required");
  need(phase === "source" || phase === "restored", "Unknown recovery phase");
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
    page.setDefaultTimeout(30000);
    // Browser writes are restricted to normal identity operations and the explicit source backup.
    await page.route("**/api/**", async route => {
      const r = route.request(), path = new URL(r.url()).pathname;
      if (!["GET", "HEAD"].includes(r.method()) && !path.startsWith("/api/auth/") &&
          !(phase === "source" && path === "/api/workspace/backup")) {
        await route.abort(); throw Error("Recovery attempted a non-identity business mutation");
      }
      await route.continue();
    });
    const failures: string[] = [];
    page.on("pageerror", () => failures.push("browser page error"));
    async function password(email: string, password: string) {
      await page.goto(base);
      await page.getByLabel("E-mail", { exact: true }).fill(email);
      await page.getByLabel("Adgangskode", { exact: true }).fill(password);
      await page.getByRole("button", { name: "Log ind", exact: true }).click();
    }
    async function login(email: string, secret: string, code: string) {
      need(code, "Unused ordinary recovery code required");
      await password(email, secret);
      await page.getByRole("button", { name: "Brug recovery code" }).click();
      await page.getByLabel("Recovery code", { exact: true }).fill(code);
      await page.getByRole("button", { name: "Bekræft", exact: true }).click();
      await page.locator('a[href="/companies/synthetic/posteringer"]').waitFor();
    }
    async function get(resource: string) {
      const r = await page.request.get(`${base}/api/companies/synthetic/${resource}`);
      need(r.ok(), `Recovery read ${resource} refused HTTP${r.status()}`);
      return r.json();
    }
    async function snapshot() {
      const entries = (await get("journal")).journal.entries;
      need(entries.length === 1 && entries[0].text === "Synthetic opening transfer", "Exact one source journal required");
      need(entries[0].lines.length === 2 &&
        entries[0].lines.some((l: any) => l.accountNo === "1800" && l.debit === 125.5 && l.credit === 0) &&
        entries[0].lines.some((l: any) => l.accountNo === "8000" && l.credit === 125.5 && l.debit === 0), "Exact balanced source lines required");
      const documents = (await get("documents")).documents.documents;
      need(documents.length === 1, "Exact original inventory required");
      const originals = [];
      for (const doc of documents) {
        const r = await page.request.get(`${base}/api/companies/synthetic/documents/${doc.id}/file`);
        need(r.ok(), "Original bytes unavailable");
        const bytes = await r.body();
        originals.push({ id: doc.id, bytesBase64: bytes.toString("base64"), sha256: hash(bytes) });
      }
      return { entries, documents, originals, trialBalance: (await get("trial-balance")).trialBalance,
        balance: (await get("balance")).balance, incomeStatement: (await get("income-statement")).incomeStatement };
    }
    if (phase === "source") {
      const state = JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!, "utf8"));
      // backup was consumed by existing reopen; backup2 is the actual unused second code.
      need(typeof state.backup2 === "string" && typeof state.postedBody === "string", "Actual owner fixture state missing");
      await login("owner@example.invalid", "synthetic-password-123", state.backup2);
      const checkpoint = await snapshot();
      save("checkpoint.json", checkpoint);
      const cookies = await page.context().cookies(base);
      need(cookies.some(c => c.httpOnly), "Current ordinary owner session missing");
      save("old-session.json", cookies.map(c => `${c.name}=${c.value}`).join("; "));
      await page.goto(`${base}/companies/synthetic/integritet`);
      const card = page.locator(".native-backup");
      await card.getByLabel("Jeg vil hente en backup af hele workspacet.", { exact: true }).check();
      const [download, response] = await Promise.all([
        page.waitForEvent("download"),
        page.waitForResponse(r => new URL(r.url()).pathname === "/api/workspace/backup" && r.request().method() === "POST"),
        card.getByRole("button", { name: "Dan og download backup", exact: true }).click(),
      ]);
      need(response.ok() && response.headers()["content-type"] === "application/vnd.rentemester.native-backup+json", "Signed native export refused");
      await download.saveAs(join(work, "signed.native-backup.json"));
      need(await download.failure() === null, "Signed download incomplete");
      need(isDeepStrictEqual(await snapshot(), checkpoint), "Export changed source checkpoint");
      save("source-proof.json", { normalOwnerMfaExport: true, exactSourceCheckpoint: true });
    } else {
      // Old credentials are sent ONLY in an isolated negative probe, never installed into the fresh browser.
      const oldCookie = JSON.parse(readFileSync(join(work, "old-session.json"), "utf8"));
      const probe = await request.newContext({ extraHTTPHeaders: { cookie: oldCookie } });
      try {
        const denied = await probe.get(`${base}/api/companies/synthetic/journal`);
        need([401, 403].includes(denied.status()), "Old source session crossed restored boundary");
      } finally { await probe.dispose(); }
      const identity = JSON.parse(readFileSync(join(work, "target-owner.json"), "utf8"));
      await page.goto(process.env.RM_CONTAINER_RECOVERY_VERIFY!.trim());
      await password(identity.email, identity.password);
      await page.getByRole("heading", { name: "Opsæt totrinsbekræftelse" }).waitFor();
      await page.getByLabel("Adgangskode", { exact: true }).fill(identity.password);
      await page.getByRole("button", { name: "Start opsætning" }).click();
      const key = await page.getByLabel("Opsætningsnøgle").inputValue();
      await page.getByLabel("Engangskode", { exact: true }).fill(totp(key));
      await page.getByRole("button", { name: "Bekræft opsætning" }).click();
      await page.getByRole("region", { name: "Recovery codes" }).locator("li").first().waitFor();
      const codes = await page.getByRole("region", { name: "Recovery codes" }).locator("li").allTextContents();
      need(codes.length >= 2, "Fresh restored MFA codes missing");
      save("target-mfa.json", { codes });
      await page.getByRole("button", { name: "Jeg har gemt koderne", exact: true }).click();
      await page.locator('a[href="/companies/synthetic/posteringer"]').waitFor();
      await page.context().clearCookies();
      await new Promise(resolve => setTimeout(resolve, 11000));
      await login(identity.email, identity.password, codes[0]);
      const checkpoint = JSON.parse(readFileSync(join(work, "checkpoint.json"), "utf8"));
      need(isDeepStrictEqual(await snapshot(), checkpoint), "Restored journal/lines/reports/original bytes differ from source");
      for (const [route, title] of [["posteringer", "Synthetic opening transfer"], ["saldobalance", "Saldobalancen stemmer — debet = kredit."]]) {
        await page.goto(`${base}/companies/synthetic/${route}`);
        await page.getByText(title, { exact: true }).waitFor();
      }
      await page.screenshot({ path: join(work, "restored.png"), fullPage: true });
      need(failures.length === 0, "Browser readback errors");
      save("restored-proof.json", { freshIdentityVerificationMfa: true, ordinaryPasswordUnusedRecoveryLogin: true,
        oldSessionRefused: true, exactJournalLinesAmountsReportsAndOriginalBytes: true, noFinancialOrProviderPost: true,
        originalSha256: checkpoint.originals.map((original: any) => original.sha256),
        checkpointSha256: hash(readFileSync(join(work, "checkpoint.json"))) });
    }
  } finally { await browser.close(); }
}

function totp(secret: string) {
  let bits = 0, count = 0; const bytes: number[] = [];
  for (const char of secret) {
    const value = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".indexOf(char);
    need(value >= 0, "Invalid synthetic MFA key");
    bits = (bits << 5) | value; count += 5;
    if (count >= 8) { count -= 8; bytes.push((bits >>> count) & 255); }
  }
  const counter = Buffer.alloc(8); counter.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 30000)));
  const digest = createHmac("sha1", Buffer.from(bytes)).update(counter).digest(), offset = digest[19] & 15;
  return String((digest.readUInt32BE(offset) & 0x7fffffff) % 1000000).padStart(6, "0");
}
