import { defineConfig, devices } from "@playwright/test";

const port = Number(process.env.RENTEMESTER_BROWSER_TEST_PORT ?? "5379");
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/*.spec.ts",
  fullyParallel: true,
  workers: process.env.CI ? 2 : 4,
  retries: 0,
  timeout: 30_000,
  expect: { timeout: 5_000 },
  reporter: [["list"], ["html", { outputFolder: "e2e/artifacts/report", open: "never" }]],
  outputDir: "e2e/artifacts/results",
  use: { baseURL, trace: "retain-on-failure", screenshot: "only-on-failure", serviceWorkers: "block" },
  webServer: {
    command: "bun scripts/browser-test-server.ts",
    url: `${baseURL}/__browser_test_ready`,
    reuseExistingServer: false,
    gracefulShutdown: { signal: "SIGTERM", timeout: 1_000 },
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 900 } } },
    { name: "webkit", testMatch: ["**/core-workflows.spec.ts", "**/tasks-workflows.spec.ts"], use: { ...devices["Desktop Safari"], viewport: { width: 1280, height: 900 } } },
  ],
});
