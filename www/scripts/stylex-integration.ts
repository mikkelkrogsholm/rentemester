import type { AstroIntegration } from "astro";
import { compileWebsite, websiteStyleSources } from "./stylex-build";

export default function websiteStylex(compile = compileWebsite, sources = websiteStyleSources): AstroIntegration {
  return {
    name: "rentemester-stylex",
    hooks: {
      "astro:config:setup": async ({ updateConfig }) => {
        await compile();
        updateConfig({ vite: { plugins: [{
          name: "rentemester-stylex-watch",
          configureServer(server) {
            let timer: ReturnType<typeof setTimeout> | undefined;
            let queue = Promise.resolve();
            server.watcher.add(sources);
            const schedule = (path: string) => {
              if (!sources.includes(path)) return;
              clearTimeout(timer);
              timer = setTimeout(() => {
                queue = queue.then(async () => {
                  try {
                    await compile();
                    server.ws.send({ type: "full-reload" });
                  } catch (error) {
                    server.config.logger.error(`StyleX: ${error instanceof Error ? error.message : String(error)}\nKeeping the last consistent stylesheet and attributes.`);
                  }
                });
              }, 60);
            };
            server.watcher.on("change", schedule);
            server.httpServer?.once("close", () => { clearTimeout(timer); server.watcher.off("change", schedule); });
          },
        }] } });
      },
    },
  };
}
