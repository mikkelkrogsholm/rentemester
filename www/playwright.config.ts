import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  fullyParallel: true,
  workers: 4,
  reporter: "list",
  use: { baseURL: "http://127.0.0.1:4176", trace: "retain-on-failure" },
  projects: [
    { name: "chromium-desktop", use: { browserName: "chromium", viewport: { width: 1440, height: 900 } } },
    { name: "chromium-mobile", use: { browserName: "chromium", viewport: { width: 390, height: 844 }, hasTouch: true } },
    { name: "webkit-desktop", use: { browserName: "webkit", viewport: { width: 1440, height: 900 } } },
    { name: "webkit-mobile", use: { browserName: "webkit", viewport: { width: 390, height: 844 }, hasTouch: true } },
  ],
  webServer: [
    { command: "bun run preview --host 127.0.0.1 --port 4176", url: "http://127.0.0.1:4176", reuseExistingServer: false, timeout: 30_000 },
    { command: "bun run dev --host 127.0.0.1 --port 4177", url: "http://127.0.0.1:4177", reuseExistingServer: false, timeout: 30_000 },
  ],
});
