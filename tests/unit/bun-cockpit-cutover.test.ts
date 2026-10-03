import { existsSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "bun:test";

const root = join(import.meta.dir, "../..");
const app = join(root, "app");

describe("cockpit Bun 1.4 cutover", () => {
  test("uses Bun directly for build, dev and isolated component tests", async () => {
    const packageJson = await Bun.file(join(app, "package.json")).json();
    expect(packageJson.engines).toEqual({ bun: "1.4.0" });
    expect(packageJson.scripts).toMatchObject({
      dev: "bun --hot scripts/serve.ts",
      build: "bun run design:check && tsc --noEmit && bun run scripts/build.ts",
      test: "bun test --isolate",
      "test:parallel": "bun test --isolate --timeout=20000 --only-failures",
    });
    for (const name of ["vite", "vitest", "@vitejs/plugin-react"]) {
      expect(packageJson.dependencies?.[name]).toBeUndefined();
      expect(packageJson.devDependencies?.[name]).toBeUndefined();
    }
  });

  test("has no Vite config, Vitest imports or installed lockfile nodes", async () => {
    expect(existsSync(join(app, "vite.config.ts"))).toBe(false);
    expect(existsSync(join(app, "vitest.config.ts"))).toBe(false);

    const imports: string[] = [];
    for await (const file of new Bun.Glob("src/**/*.{ts,tsx}").scan({ cwd: app, absolute: true })) {
      if ((await Bun.file(file).text()).includes('from "vitest"')) imports.push(file);
    }
    expect(imports).toEqual([]);

    expect(existsSync(join(app, "bun.lock"))).toBe(false);
    const rootPackageJson = await Bun.file(join(root, "package.json")).json();
    expect(rootPackageJson.workspaces).toEqual(["app"]);
    expect(rootPackageJson.scripts).toMatchObject({
      "cockpit:dev": "bun run --filter rentemester-cockpit dev",
      "cockpit:test": "bun run --filter rentemester-cockpit test",
      "cockpit:test:parallel": "bun run --filter rentemester-cockpit test:parallel",
      "cockpit:build": "bun run --filter rentemester-cockpit build",
    });

    const lock = await Bun.file(join(root, "bun.lock")).text();
    expect(lock).toContain('"rentemester-cockpit@workspace:app"');
    expect(lock).not.toMatch(/^\s+"(?:vite|vitest|@vitest\/[^\"]+)"\s*:/m);
  });

  test("keeps the Bun build and dev proxy explicit", async () => {
    const build = await Bun.file(join(app, "scripts/build.ts")).text();
    const serve = await Bun.file(join(app, "scripts/serve.ts")).text();
    expect(build).toContain("Bun.build");
    expect(serve).toContain("Bun.serve");
    expect(serve).toContain('"/api/*"');
    expect(serve).toContain('"/*": cockpit');
  });
});
