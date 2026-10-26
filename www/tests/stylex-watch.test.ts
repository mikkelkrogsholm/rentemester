import { expect, test } from "bun:test";
import { EventEmitter } from "node:events";
import websiteStylex from "../scripts/stylex-integration";

const settle = () => new Promise(resolve => setTimeout(resolve, 100));

test("dev watcher debounces, serializes, retains consistent output after errors, and recovers", async () => {
  let calls = 0;
  let concurrent = 0;
  let maximumConcurrent = 0;
  let consistentGeneration = 0;
  let fail = false;
  const compile = async () => {
    calls++;
    maximumConcurrent = Math.max(maximumConcurrent, ++concurrent);
    try {
      await new Promise(resolve => setTimeout(resolve, 30));
      if (fail) throw new Error("invalid source");
      consistentGeneration++;
    } finally { concurrent--; }
  };
  let plugin: any;
  const integration = websiteStylex(compile, ["/site.stylex.ts", "/DESIGN.md"]);
  await (integration.hooks["astro:config:setup"] as any)({ updateConfig: (config: any) => { plugin = config.vite.plugins[0]; } });
  const watcher = new EventEmitter() as EventEmitter & { add: (paths: string[]) => void };
  watcher.add = paths => expect(paths).toEqual(["/site.stylex.ts", "/DESIGN.md"]);
  const serverClose = new EventEmitter();
  const messages: any[] = [];
  const errors: string[] = [];
  plugin.configureServer({ watcher, httpServer: serverClose, ws: { send: (value: any) => messages.push(value) }, config: { logger: { error: (value: string) => errors.push(value) } } });
  for (let n = 0; n < 10; n++) watcher.emit("change", "/site.stylex.ts");
  await settle();
  expect(calls).toBe(2);
  expect(consistentGeneration).toBe(2);
  expect(messages).toEqual([{ type: "full-reload" }]);
  fail = true;
  watcher.emit("change", "/DESIGN.md");
  await settle();
  expect(consistentGeneration).toBe(2);
  expect(messages).toHaveLength(1);
  expect(errors[0]).toContain("Keeping the last consistent stylesheet and attributes");
  fail = false;
  watcher.emit("change", "/site.stylex.ts");
  await settle();
  expect(consistentGeneration).toBe(3);
  expect(messages).toHaveLength(2);
  expect(maximumConcurrent).toBe(1);
  serverClose.emit("close");
  watcher.emit("change", "/site.stylex.ts");
  await settle();
  expect(calls).toBe(4);
});
