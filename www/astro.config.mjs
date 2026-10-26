import { defineConfig } from "astro/config";
import sitemap from "@astrojs/sitemap";
import websiteStylex from "./scripts/stylex-integration";

export default defineConfig({
  site: "https://rentemester.dk",
  trailingSlash: "never",
  build: {
    format: "file",
  },
  integrations: [
    websiteStylex(),
    sitemap({
      i18n: { defaultLocale: "da", locales: { da: "da-DK" } },
    }),
  ],
});
