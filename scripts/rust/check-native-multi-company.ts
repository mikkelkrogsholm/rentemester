import { createHmac } from "node:crypto";
import { mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { chromium, type Page } from "@playwright/test";
import { checkNativeLostPosting } from "./check-native-lost-posting";
import { checkNativeAccessLoss } from "./check-native-access-loss";
import { verifyNativeServiceSalesRestart } from "./check-native-service-sales";
import { checkNativeServiceCompany } from "./check-native-service-company";
import { checkNativeContextSwitch } from "./check-native-context-switch";

const saved = "/tmp/rentemester-rust-reports/native-multi-company.json";
const second = "synthetic-second-ledger";
const label = (slug: string) => `Synthetic isolated company ${slug}`;
type Evidence = {
  primaryUuid: string;
  secondUuid: string;
  primaryYear: string;
  secondYear: string;
  restarted: boolean;
};
/** Own second company and identical account numbers in distinct real ledgers. */
export async function checkNativeMultiCompany(page: Page, base: string, restarted = false) {
  const headers = { origin: base, "content-type": "application/json" };
  async function get(slug: string, resource: string) {
    const r = await page.request.get(`${base}/api/companies/${slug}/${resource}`);
    if (!r.ok()) throw new Error(`Multi-company ${slug}/${resource}: ${await r.text()}`);
    return r.json();
  }
  async function post(slug: string, resource: string, data: unknown) {
    const r = await page.request.post(`${base}/api/companies/${slug}/${resource}`, {
      headers,
      data,
    });
    if (!r.ok()) throw new Error(`Multi-company ${slug}/${resource}: ${await r.text()}`);
    return r.json();
  }
  let evidence: Evidence;
  if (!restarted) {
    const before = (await get("synthetic", "journal")).journal;
    await page.goto(`${base}/companies/new`);
    const form = page.getByRole("form", { name: "Opret virksomhed", exact: true });
    await form.getByLabel("Virksomhedsnavn", { exact: true }).fill("Synthetic Separate Company");
    await form.getByText("Avancerede indstillinger", { exact: true }).click();
    await form.getByLabel(/^Slug \(valgfrit\)/).fill(second);
    await form.getByLabel(/^Regnskabsår starter i måned/).selectOption("4");
    await form.getByLabel(/^Momsperiode/).selectOption("none");
    const [created] = await Promise.all([
      page.waitForResponse(
        (r) => new URL(r.url()).pathname === "/api/companies" && r.request().method() === "POST",
      ),
      form.getByRole("button", { name: "Opret virksomhed", exact: true }).click(),
    ]);
    if (created.status() !== 201)
      throw new Error(`CompanyForm create failed: ${await created.text()}`);
    await page.waitForURL(`**/companies/${second}`);
    const me = await (await page.request.get(`${base}/api/me`)).json();
    if (!me.companies.some((c: any) => c.slug === second && c.role === "owner"))
      throw new Error("Creator membership missing from current context");
    const fresh = (await get(second, "journal")).journal;
    if (fresh.entries.length !== 0 || fresh.nativePosting.accounts.length !== 0)
      throw new Error("New ledger borrowed accounting or chart");
    if (fresh.ledgerUuid === before.ledgerUuid || !/^[a-f0-9]{32}$/.test(fresh.ledgerUuid))
      throw new Error("Company ledgers share identity");
    if (JSON.stringify((await get("synthetic", "journal")).journal) !== JSON.stringify(before))
      throw new Error("Creating company changed original journal");
    const duplicate = await page.request.post(`${base}/api/companies`, {
      headers,
      data: { name: "Duplicate synthetic", slug: second },
    });
    if (duplicate.status() !== 409) throw new Error("Registered company slug reused");
    for (const slug of ["synthetic", second]) {
      await page.goto(`${base}/companies/${slug}/kontoplan`);
      const account = page.getByRole("form", { name: "Opret konto", exact: true });
      for (const [no, type, normal] of [
        ["1759", "asset", "debit"],
        ["8759", "equity", "credit"],
      ]) {
        await account.getByLabel("Kontonummer", { exact: true }).fill(no);
        await account.getByLabel("Kontonavn", { exact: true }).fill(`${label(slug)} ${no}`);
        await account.getByLabel(/^Kontotype/).selectOption(type);
        await account.getByLabel(/^Normal saldo/).selectOption(normal);
        await account.getByRole("button", { name: "Opret konto", exact: true }).click();
        await page
          .getByRole("row")
          .filter({ hasText: `${label(slug)} ${no}` })
          .waitFor();
      }
      const journal = (await get(slug, "journal")).journal;
      const amount = slug === second ? "56.78" : "12.34";
      await post(slug, "manual-journal", {
        confirm: true,
        input: {
          format: "rentemester-manual-journal-post/1",
          expectedRuleVersion: journal.nativePosting.ruleVersion,
          idempotencyKey: "synthetic-multi-company-same-key-separate-ledgers",
          request: {
            transactionDate: new Date().toLocaleDateString("sv-SE", {
              timeZone: "Europe/Copenhagen",
            }),
            text: label(slug),
            currency: "DKK",
            lines: [
              { accountNo: "1759", debitAmount: amount, creditAmount: "0.00" },
              { accountNo: "8759", debitAmount: "0.00", creditAmount: amount },
            ],
          },
        },
      });
    }
    const a = (await get("synthetic", "journal")).journal;
    const b = (await get(second, "journal")).journal;
    evidence = {
      primaryUuid: a.ledgerUuid,
      secondUuid: b.ledgerUuid,
      primaryYear: a.selectedYear,
      secondYear: b.selectedYear,
      restarted: false,
    };
  } else evidence = JSON.parse(readFileSync(saved, "utf8"));
  for (let i = 0; i < 4; i++) {
    const journals = await Promise.all([
      get("synthetic", `journal?year=${encodeURIComponent(evidence.primaryYear)}`),
      get(second, `journal?year=${encodeURIComponent(evidence.secondYear)}`),
    ]);
    for (const [index, slug, expected, uuid] of [
      [0, "synthetic", 12.34, evidence.primaryUuid],
      [1, second, 56.78, evidence.secondUuid],
    ] as const) {
      const j = journals[index].journal;
      const own = j.entries.filter((e: any) => e.text === label(slug));
      if (
        j.slug !== slug ||
        j.ledgerUuid !== uuid ||
        own.length !== 1 ||
        own[0].total !== expected ||
        j.entries.some((e: any) => e.text === label(slug === second ? "synthetic" : second))
      )
        throw new Error("Cross-company journal selection/accounting leakage");
    }
  }
  for (const [slug, year, expected] of [
    ["synthetic", evidence.primaryYear, 12.34],
    [second, evidence.secondYear, 56.78],
  ] as const) {
    const balance = (await get(slug, `balance?year=${encodeURIComponent(year)}`)).balance;
    if (balance.assets.lines.find((l: any) => l.accountNo === "1759")?.amount !== expected)
      throw new Error("Separate real report amount missing");
    await page.goto(`${base}/companies/${slug}/posteringer?year=${encodeURIComponent(year)}`);
    await page.getByText(label(slug), { exact: true }).waitFor();
    await page.goto(`${base}/`);
    await page.getByLabel("Skift virksomhed", { exact: true }).selectOption(slug);
    await page.waitForURL(`**/companies/${slug}`);
  }
  const profile = (await get(second, "company")).company;
  if (profile.fiscalYearStartMonth !== 4 || profile.vatPeriodType !== null)
    throw new Error("Explicit company fiscal/VAT settings lost");
  evidence.restarted = restarted;
  mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
  writeFileSync(saved, JSON.stringify(evidence, null, 2), { mode: 0o600 });
}

// Standalone normal verification/MFA qualification, restricted to its owned synthetic workspace.
async function standalone() {
  const work = resolve(process.env.RM_MULTI_WORK ?? "");
  if (
    !basename(work).startsWith("Rentemester-Synthetic-Multi-") ||
    !statSync(work).isDirectory() ||
    (statSync(work).mode & 0o077) !== 0
  )
    throw new Error("Owned private synthetic fixture required");
  const config = JSON.parse(readFileSync(join(work, "server.json"), "utf8"));
  if (
    config.workspace !== "synthetic-native-multi" ||
    resolve(config.root) !== join(work, "workspace")
  )
    throw new Error("Synthetic fixture only");
  const base = process.env.RM_MULTI_BASE!;
  if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(base)) throw new Error("Literal loopback only");
  const spool = join(config.root, ".rentemester");
  const files = (prefix: string) =>
    readdirSync(spool).filter((n) => n.startsWith(prefix) && n.endsWith(".txt"));
  const link = (prefix: string, prior: string[] = []) => {
    const added = files(prefix).filter((n) => !prior.includes(n));
    if (added.length !== 1) throw new Error("Expected one private delivery");
    return readFileSync(join(spool, added[0]), "utf8").trim();
  };
  const browser = await chromium.launch({ headless: true });
  const owner = await browser.newPage();
  const reader = await browser.newPage();
  owner.setDefaultTimeout(45000);
  reader.setDefaultTimeout(45000);
  const ownerPassword = "synthetic-owner-password-123",
    readerPassword = "synthetic-reader-password-456";
  const privateState = join(work, "browser-state.json");
  const totpSecrets: Record<string, string> = {};
  function totp(secret: string) {
    let bits = 0,
      count = 0;
    const bytes: number[] = [];
    for (const c of secret) {
      const v = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".indexOf(c);
      if (v < 0) throw new Error("Invalid synthetic MFA key");
      bits = (bits << 5) | v;
      count += 5;
      if (count >= 8) {
        count -= 8;
        bytes.push((bits >>> count) & 255);
      }
    }
    const counter = Buffer.alloc(8);
    counter.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 30000)));
    const d = createHmac("sha1", Buffer.from(bytes)).update(counter).digest();
    const o = d[19] & 15;
    return String((d.readUInt32BE(o) & 0x7fffffff) % 1000000).padStart(6, "0");
  }
  async function login(page: Page, email: string, password: string, recovery: string) {
    await page.goto(base);
    await page.getByLabel("E-mail", { exact: true }).fill(email);
    await page.getByLabel("Adgangskode", { exact: true }).fill(password);
    await page.getByRole("button", { name: "Log ind", exact: true }).click();
    await page.getByRole("button", { name: "Brug recovery code" }).click();
    await page.getByLabel("Recovery code", { exact: true }).fill(recovery);
    await page.getByRole("button", { name: "Bekræft", exact: true }).click();
    await page.getByRole("link", { name: "Åbn bogføring" }).first().waitFor();
  }
  async function enroll(page: Page, email: string, password: string) {
    await page.goto(base);
    await page.getByLabel("E-mail", { exact: true }).fill(email);
    await page.getByLabel("Adgangskode", { exact: true }).fill(password);
    await page.getByRole("button", { name: "Log ind", exact: true }).click();
    await page.getByRole("heading", { name: "Opsæt totrinsbekræftelse" }).waitFor();
    await page.getByLabel("Adgangskode", { exact: true }).fill(password);
    await page.getByRole("button", { name: "Start opsætning", exact: true }).click();
    const secret = await page.getByLabel("Opsætningsnøgle").inputValue();
    totpSecrets[email] = secret;
    await page.getByLabel("Engangskode", { exact: true }).fill(totp(secret));
    await page.getByRole("button", { name: "Bekræft opsætning", exact: true }).click();
    await page.getByRole("region", { name: "Recovery codes" }).locator("li").first().waitFor();
    const codes = await page
      .getByRole("region", { name: "Recovery codes" })
      .locator("li")
      .allTextContents();
    if (codes.length < 2) throw new Error("Recovery codes missing");
    await page.getByRole("button", { name: "Jeg har gemt koderne", exact: true }).click();
    await page.getByRole("link", { name: "Åbn bogføring" }).first().waitFor();
    await page.context().clearCookies();
    await new Promise((r) => setTimeout(r, 11000));
    await login(page, email, password, codes[0]);
    return codes[1];
  }
  const headers = { origin: base, "content-type": "application/json" };
  async function permissions() {
    const own = await reader.request.get(`${base}/api/companies/${second}/journal`);
    const foreign = await reader.request.get(`${base}/api/companies/synthetic/journal`);
    if (!own.ok() || foreign.ok()) throw new Error("Per-company membership not enforced");
    const denied = await reader.request.post(`${base}/api/companies`, {
      headers,
      data: { name: "Forbidden company", slug: "synthetic-forbidden-company" },
    });
    if (denied.status() !== 403) throw new Error("Member created company");
    const write = await reader.request.post(`${base}/api/companies/${second}/accounts`, {
      headers,
      data: { accountNo: "1999", name: "Forbidden", type: "asset", normalBalance: "debit" },
    });
    if (write.ok()) throw new Error("Reader wrote company accounts");
    const list = await (await reader.request.get(`${base}/api/companies`)).json();
    if (list.companies.length !== 1 || list.companies[0].slug !== second)
      throw new Error("Catalog leaked ungranted company");
    const ownerRead = await owner.request.get(`${base}/api/companies/${second}/journal`);
    if (!ownerRead.ok()) throw new Error("Permission refusal retired owner");
  }
  try {
    if (process.argv.includes("--service-sales-restart")) {
      const state = JSON.parse(readFileSync(privateState, "utf8"));
      await owner.goto(base);
      await owner.getByLabel("E-mail", { exact: true }).fill("owner@example.invalid");
      await owner.getByLabel("Adgangskode", { exact: true }).fill(ownerPassword);
      await owner.getByRole("button", { name: "Log ind", exact: true }).click();
      await owner.getByLabel("Engangskode", { exact: true }).fill(totp(state.ownerTotp));
      await owner.getByRole("button", { name: "Bekræft", exact: true }).click();
      await owner.getByRole("link", { name: "Åbn bogføring" }).first().waitFor();
      await verifyNativeServiceSalesRestart(owner, base);
      console.log("PASS: service customer/invoice/original/journal persist after second restart and normal MFA");
      return;
    }
    if (process.argv.includes("--reopen")) {
      const state = JSON.parse(readFileSync(privateState, "utf8"));
      await login(owner, "owner@example.invalid", ownerPassword, state.ownerRecovery);
      await login(reader, "reader@example.invalid", readerPassword, state.readerRecovery);
      await permissions();
      await checkNativeMultiCompany(owner, base, true);
    } else {
      await owner.goto(link("verification-"));
      const ownerRecovery = await enroll(owner, "owner@example.invalid", ownerPassword);
      await checkNativeMultiCompany(owner, base);
      const before = files("invitation-");
      const issue = await owner.request.post(`${base}/api/workspace/invitations`, {
        headers,
        data: {
          email: "reader@example.invalid",
          workspaceRole: "member",
          companySlug: second,
          companyRole: "reader",
        },
      });
      if (!issue.ok())
        throw new Error(`Second-company normal invitation failed: ${await issue.text()}`);
      const verifyBefore = files("verification-");
      await reader.goto(link("invitation-", before));
      await reader.getByLabel("Navn", { exact: true }).fill("Synthetic isolated reader");
      await reader.getByLabel("Adgangskode", { exact: true }).fill(readerPassword);
      await reader.getByRole("button", { name: "Acceptér invitation", exact: true }).click();
      await reader.getByText("Invitationen er accepteret.", { exact: true }).waitFor();
      await reader.goto(link("verification-", verifyBefore));
      const readerRecovery = await enroll(reader, "reader@example.invalid", readerPassword);
      await permissions();
      writeFileSync(privateState, JSON.stringify({ ownerRecovery, readerRecovery, ownerTotp: totpSecrets["owner@example.invalid"], readerTotp: totpSecrets["reader@example.invalid"] }), {
        mode: 0o600,
      });
    }
    const result = JSON.parse(readFileSync(saved, "utf8"));
    result.normalVerificationAndMfa = true;
    result.currentPerCompanyPermissions = true;
    result.noOfflineIdentitySeed = true;
    if (process.argv.includes("--reopen")) result.lostPostingReply = await checkNativeLostPosting(owner, base);
    if (process.argv.includes("--reopen")) result.pendingAccessLoss = await checkNativeAccessLoss(owner, reader, base);
    if (process.argv.includes("--reopen") && process.env.RM_MULTI_CLI && process.env.RM_MULTI_MCP) result.serviceCompany = await checkNativeServiceCompany(owner, base, work, process.env.RM_MULTI_CLI, process.env.RM_MULTI_MCP);
    if (process.argv.includes("--reopen")) result.contextSwitch = await checkNativeContextSwitch(owner, base, async () => {
      const { readerTotp } = JSON.parse(readFileSync(privateState, "utf8"));
      await owner.getByLabel("E-mail", { exact: true }).fill("reader@example.invalid");
      await owner.getByLabel("Adgangskode", { exact: true }).fill(readerPassword);
      await owner.getByRole("button", { name: "Log ind", exact: true }).click();
      await owner.getByLabel("Engangskode", { exact: true }).fill(totp(readerTotp));
      await owner.getByRole("button", { name: "Bekræft", exact: true }).click();
      await owner.getByRole("group", { name: "Konto", exact: true }).getByText("reader@example.invalid", { exact: true }).waitFor();
    });
    writeFileSync(saved, JSON.stringify(result, null, 2), { mode: 0o600 });
    console.log(
      "PASS: ordinary second-company creation, independent ledgers/reports/current permissions" +
        (process.argv.includes("--reopen") ? " after restart" : ""),
    );
  } catch (e) {
    await owner.screenshot({
      path: "/tmp/rentemester-rust-reports/native-multi-company-failure.png",
      fullPage: true,
    });
    console.error(await owner.locator("body").innerText());
    throw e;
  } finally {
    await browser.close();
  }
}
if (import.meta.main) await standalone();
