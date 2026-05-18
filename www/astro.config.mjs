import { defineConfig } from "astro/config";
import sitemap from "@astrojs/sitemap";
import tailwind from "@astrojs/tailwind";

export default defineConfig({
  site: "https://rentemester.dk",
  trailingSlash: "never",
  build: {
    format: "file",
  },
  integrations: [
    tailwind({ applyBaseStyles: false }),
    sitemap({
      i18n: { defaultLocale: "da", locales: { da: "da-DK" } },
    }),
  ],
});
