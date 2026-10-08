import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test("landing keeps compiled stylesheet, identity, responsive layout and SEO", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("En åben AI-bogholderfor dansk bogføring");
  const stylesheet = page.locator('link[rel="stylesheet"]');
  await expect(stylesheet).toHaveCount(1);
  await expect(stylesheet).toHaveAttribute("href", /^\/_stylex\/site-[a-f0-9]{16}\.css$/);
  const css = await (await page.request.get(await stylesheet.getAttribute("href") ?? "")).text();
  expect(css).not.toContain("@layer");
  expect(css).not.toContain("--tw-");
  await expect(page.locator("body")).toHaveCSS("background-color", "rgb(5, 7, 10)");
  await expect(page.getByRole("heading", { level: 1 })).toHaveCSS("font-family", /Georgia/);
  expect(await page.locator('link[rel="canonical"]').getAttribute("href")).toBe("https://rentemester.dk/");
  expect(await page.locator('script[type="application/ld+json"]').count()).toBeGreaterThan(3);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  expect(await page.locator('script[src*="stylex"]').count()).toBe(0);
  const result = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa"]).analyze();
  expect(result.violations).toEqual([]);
});

test("mobile menu uses native hidden state and keyboard controls", async ({ page }) => {
  await page.goto("/");
  const button = page.locator("#mobile-nav-toggle");
  const mobile = (page.viewportSize()?.width ?? 1440) < 768;
  if (!mobile) {
    await expect(button).toBeHidden();
    await expect(page.getByRole("navigation", { name: "Hovedmenu" })).toBeVisible();
    await expect(page.getByRole("navigation", { name: "Mobil-menu" })).toBeHidden();
    return;
  }
  const nav = page.getByRole("navigation", { name: "Mobil-menu" });
  await expect(nav).toBeHidden();
  await button.focus();
  await page.keyboard.press("Enter");
  await expect(button).toHaveAttribute("aria-expanded", "true");
  await expect(nav).toBeVisible();
  await expect(nav.getByRole("link", { name: "Funktioner", exact: true })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(nav).toBeHidden();
  await expect(button).toHaveAttribute("aria-expanded", "false");
});

test("article typography styles formerly unclassed descendants", async ({ page }) => {
  await page.goto("/viden/moms/satser");
  const heading = page.getByRole("heading", { level: 2 }).first();
  await expect(heading).toHaveCSS("font-size", "32px");
  await expect(heading).toHaveCSS("margin-top", "48px");
  const article = page.locator("article").first();
  await expect(article.locator("p").first()).toHaveCSS("margin-bottom", "20px");
  await expect(article.locator("a").first()).toHaveCSS("color", "rgb(0, 209, 255)");
  await expect(page.getByRole("navigation", { name: "Brødkrumme" })).toBeVisible();
});

test("code blocks stay readable, scroll within viewport, and reduced motion works", async ({ page }) => {
  await page.goto("/docs/installation");
  const pre = page.locator("pre").first();
  await expect(pre).toHaveCSS("overflow-x", "auto");
  await expect(pre).toHaveCSS("background-color", "rgb(11, 14, 20)");
  await expect(pre.locator("code")).toHaveCSS("padding-top", "0px");
  await expect(pre.locator("code")).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await expect(page.getByRole("link", { name: "Se projektet", exact: true })).toHaveCSS("transition-duration", "0s");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test("calculator remains interactive in both VAT directions", async ({ page }) => {
  await page.goto("/vaerktoej/momsberegner");
  await page.getByLabel("Beløb", { exact: true }).fill("2.000,00");
  await expect(page.locator("#result-vat")).toHaveText(/500,00/);
  await expect(page.locator("#result-gross")).toHaveText(/2.500,00/);
  await page.getByRole("radio", { name: "Inkl. moms", exact: true }).check();
  await expect(page.locator("#result-net")).toHaveText(/1.600,00/);
  await expect(page.locator("#result-vat")).toHaveText(/400,00/);
});

test("development serves compiled CSS and retains browser interactions", async ({ page }) => {
  await page.goto("http://127.0.0.1:4177/vaerktoej/momsberegner");
  const stylesheet = page.locator('link[rel="stylesheet"]');
  await expect(stylesheet).toHaveCount(1);
  await expect(stylesheet).toHaveAttribute("href", /^\/_stylex\/site-[a-f0-9]{16}\.css$/);
  await expect(page.locator("body")).toHaveCSS("background-color", "rgb(5, 7, 10)");
  await page.getByLabel("Beløb", { exact: true }).fill("400");
  await expect(page.locator("#result-vat")).toHaveText(/100,00/);
});

test("404 and skip link preserve recovery and visible keyboard focus", async ({ page }) => {
  await page.goto("/404");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await expect(page.getByRole("link", { name: /forside/i }).last()).toBeVisible();
  await page.keyboard.press("Tab");
  const skip = page.getByRole("link", { name: "Spring til indhold" });
  await skip.focus();
  await expect(skip).toBeFocused();
  await expect(skip).toBeVisible();
  await expect(skip).toHaveCSS("position", "fixed");
  await expect(skip).toHaveCSS("outline-style", "solid");
});
