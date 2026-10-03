import { expect, test } from "@playwright/test";
import companyRoutes from "./data/routes.json" with { type: "json" };
import { COMPANY_SLUG, mockApi } from "./fixtures";

const pageRoutes = companyRoutes.filter((route) => !("kind" in route) || route.kind === "page");

// Assert view-specific data/content, not just that a shell survived an error.
const routeEvidence: Record<string, RegExp> = {
  dashboard: /Omsætning/,
  attention: /Ingen opgaver kræver opmærksomhed/,
  "approval-policy": /Uafhængig reviewer/,
  "purchase-overview": /Ingen åbne grupper i perioden/,
  "party-hub": /Ingen parter endnu/,
  "income-statement": /Vareforbrug/,
  balance: /Selskabskapital/,
  "trial-balance": /Omsætning/,
  obligations: /Skyldig selskabsskat/,
  liquidity: /Pengestrøm|Indbetalinger|Indbetalt/,
  budget: /Budget/,
  journal: /Salg af ydelse/,
  drafts: /Ny kladde/,
  "posting-rules": /Posteringsregler/,
  "batch-bookkeeping": /Bogføringsarbejdsbord/,
  bank: /Gebyr/,
  vat: /Salgsmoms/,
  documents: /DOC-2026-000001/,
  payables: /Leverandør|Ubetalt/,
  invoices: /2026-00002/,
  "invoice-templates": /Ingen skabeloner/,
  contacts: /Kunde A\/S/,
  "workspace-register": /Ingen synlige parter/,
  "workspace-inbox": /Ingen synlige kilder/,
  mileage: /Kørsel|Kilometer/,
  assets: /Anlæg|Afskrivning/,
  suggestions: /Ingen.*forslag|Ingen åbne/i,
  archive: /Arkiverede regnskabsår/,
  "multi-year": /2023/,
  manage: /Gem navn|Virksomhedsnavn/,
  retention: /2031-12-31/,
  integrity: /kæden er hel/,
  accounts: /Kontorartikler/,
  dimensions: /Ingen definitioner/,
  exceptions: /Ingen.*undtagelser|Ingen åbne/,
  "period-lock": /Luk periode/,
  "bank-accounts": /synthetic-csv/,
  gdpr: /Find oplysninger/,
  accruals: /Accruals|Periodisering/,
  "annual-report": /Vælg regnskabsår/,
  "receipt-email": /Forbind til mailkonto/,
};

for (const route of pageRoutes) {
  test(`company route ${route.id} renders its real success contract`, async ({ page }) => {
    const fixture = await mockApi(page);
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    const suffix = route.segment ? `/${route.segment}` : "";
    await page.goto(`/companies/${COMPANY_SLUG}${suffix}?year=2026`);
    await expect(page.getByRole("main")).toContainText(routeEvidence[route.id]);
    await expect(page.getByRole("main").getByRole("heading").first()).toBeVisible();
    expect(errors).toEqual([]);
    fixture.assertComplete();
  });
}

for (const route of pageRoutes) {
  test(`company route ${route.id} fits the 320px mobile viewport`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 320, height: 844 });
    const fixture = await mockApi(page);
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    const suffix = route.segment ? `/${route.segment}` : "";
    await page.goto(`/companies/${COMPANY_SLUG}${suffix}?year=2026`);
    await expect(page.getByRole("main")).toContainText(routeEvidence[route.id]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
    expect(errors).toEqual([]);
    await page.screenshot({ path: testInfo.outputPath(`${route.id}-320.png`), fullPage: true });
    fixture.assertComplete();
  });
}

for (const route of [
  { path: "/", evidence: /Acme ApS/ },
  { path: "/companies/new", evidence: /Opret virksomhed/ },
  { path: "/help", evidence: /Hjælp/ },
  { path: "/lovgrundlag", evidence: /Syntetiske regler/ },
  { path: "/cfo", evidence: /Ingen kildeposteringer/ },
  { path: "/koncernstruktur", evidence: /Koncernstruktur/ },
  { path: "/adgang", evidence: /Invitation|Brugere|Medlemmer/ },
]) {
  test(`workspace route ${route.path} renders success content`, async ({ page }) => {
    const fixture = await mockApi(page, { profile: "hosted" });
    await page.goto(route.path);
    await expect(page.getByRole("main")).toContainText(route.evidence);
    fixture.assertComplete();
  });
}

test("unknown routes retain a useful return link", async ({ page }) => {
  const fixture = await mockApi(page);
  await page.goto("/missing-synthetic-page");
  await expect(page.getByRole("main")).toContainText("Siden findes ikke");
  await expect(page.getByRole("main").getByRole("link", { name: "Til porteføljen" })).toHaveAttribute("href", "/");
  fixture.assertComplete();
});
