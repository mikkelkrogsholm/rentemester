import { readFileSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { chromium, webkit } from "@playwright/test";
import { openDb, migrate } from "../src/core/db";
import { ensureCompanyDirs } from "../src/core/paths";
import { seedAccounts } from "../src/core/ledger";
import { buildAnnualReport } from "../src/core/annual-report";
import { generateIxbrl } from "../src/core/ixbrl";
import { seedHistoricalClosedPeriod } from "../tests/helpers/close-period";

const root = mkdtempSync(join(tmpdir(), "rentemester-stylex-documents-"));
const db = openDb(ensureCompanyDirs(root).db);
try {
  migrate(db);
  seedAccounts(db);
  db.query("INSERT INTO companies(id,name,country,currency,cvr,fiscal_year_start_month,fiscal_year_label_strategy) VALUES(1,'Synthetic Økonomi ApS','DK','DKK','DK10000001',1,'end-year')").run();
  seedHistoricalClosedPeriod(db, { periodStart: "2025-01-01", periodEnd: "2025-12-31", kind: "fiscal_year" });
  const ixbrl = generateIxbrl(buildAnnualReport(db, "2025-01-01", "2025-12-31"));
  if (!ixbrl.ok || !ixbrl.xhtml) throw new Error(`synthetic iXBRL fixture failed: ${ixbrl.errors.join("; ")}`);
  if (ixbrl.xhtml.includes("<script")) throw new Error("iXBRL must remain script-free");
  const documents = [
    ["dashboard", readFileSync("/tmp/rentemester-smoke/dashboard.html", "utf8")],
    ["backup guide", readFileSync("/tmp/rentemester-smoke/backup-guide.html", "utf8")],
    ["compliance report", readFileSync("/tmp/rentemester-smoke/compliance-report.html", "utf8")],
    ["iXBRL", ixbrl.xhtml],
  ] as const;
  let verified = 0;
  for (const engine of [chromium, webkit]) {
    const browser = await engine.launch();
    try {
      for (const width of [1440, 390]) {
        for (const [name, html] of documents) {
          const page = await browser.newPage({ viewport: { width, height: 900 } });
          const network: string[] = [];
          await page.route("**/*", (route) => { network.push(route.request().url()); return route.abort(); });
          await page.emulateMedia({ media: "screen", reducedMotion: "reduce" });
          if (name === "iXBRL") await page.goto(`data:application/xhtml+xml;base64,${Buffer.from(html).toString("base64")}`);
          else await page.setContent(html);
          await page.evaluate(async () => { await document.fonts.ready; });
          if (network.length) throw new Error(`${name} requested external resources: ${network.join(", ")}`);
          const body = page.locator("body");
          if (!await body.getAttribute("class")) throw new Error(`${name} has no compiled body styling`);
          if (!await page.locator("html").getAttribute("class")) throw new Error(`${name} has no compiled root styling`);
          if (!await page.getByRole("heading", { level: 1 }).isVisible()) throw new Error(`${name} has no visible title`);
          const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth + 1);
          if (overflow) throw new Error(`${name} overflows ${width}px`);
          if (name !== "iXBRL") {
            const loaded = await page.evaluate(() => document.fonts.check('400 16px "IBM Plex Sans"', "ÆØÅ æøå Żółć"));
            if (!loaded) throw new Error(`${name} offline fonts did not load`);
            const facesLoaded = await page.evaluate(() => Array.from(document.fonts).length >= 8 && Array.from(document.fonts).every(face => face.status === "loaded"));
            if (!facesLoaded) throw new Error(`${name} did not register/load its own offline font faces`);
          }
          const scroller = page.getByRole("region", { name: "Regnskabstabel" }).first();
          if (await scroller.count() && await scroller.evaluate(element => element.scrollWidth > element.clientWidth + 1)) {
            await scroller.focus();
            await page.keyboard.down("ArrowRight");
            await page.waitForTimeout(150);
            await page.keyboard.up("ArrowRight");
            try {
              await page.waitForFunction(() => document.querySelector('[role="region"][aria-label="Regnskabstabel"]')!.scrollLeft > 0, undefined, { timeout: 2000 });
            } catch (error) {
              throw new Error(`${engine.name()} ${name} ${width}px table keyboard scrolling failed: ${JSON.stringify(await scroller.evaluate(element => ({ width: element.clientWidth, total: element.scrollWidth, scroll: element.scrollLeft, active: element === document.activeElement })))}; ${error}`);
            }
          }
          await page.emulateMedia({ media: "print" });
          const paper = await body.evaluate((element) => getComputedStyle(element).backgroundColor);
          if (paper !== "rgb(255, 255, 255)") throw new Error(`${name} print surface is ${paper}`);
          verified += 1;
          await page.close();
        }
      }
    } finally { await browser.close(); }
  }
  console.log(`standalone document browsers passed: ${verified} Chromium/WebKit desktop/mobile cases, offline fonts, script-free iXBRL, root/body styling and print`);
} finally {
  db.close();
  rmSync(root, { recursive: true, force: true });
}
