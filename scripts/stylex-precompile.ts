import { createHash } from "node:crypto";

export type CompiledAttributes = Record<string, Record<string, string>>;
export type StylexCompilerDependencies = {
  transformSync: (code: string, options: Record<string, unknown>) => {
    code?: string | null;
    metadata?: Record<string, unknown>;
  } | null;
  stylexBabelPlugin: unknown;
  processStylexRules: (rules: unknown[], useLayers?: boolean) => string;
  stylexRuntime: unknown;
};

/** Compile author-owned styles once; consumers receive ordinary attributes and CSS.
 * The input is a self-contained module exporting `attributes`. Dependencies are
 * supplied by each build so the independently installed website needs no root
 * node_modules. Stable repository-relative filenames keep class hashes portable.
 */
export function compileStylex(
  source: string,
  filename: string,
  dependencies: StylexCompilerDependencies,
): { attributes: CompiledAttributes; css: string; sourceHash: string } {
  if (filename.startsWith("/") || filename.includes("..")) {
    throw new Error("StyleX compiler requires a repository-relative filename");
  }
  const compiled = dependencies.transformSync(source, {
    filename,
    babelrc: false,
    configFile: false,
    parserOpts: { sourceType: "module", plugins: ["typescript"] },
    plugins: [[dependencies.stylexBabelPlugin, {
      importSources: ["@stylexjs/stylex"],
      runtimeInjection: false,
      dev: false,
      useCSSLayers: false,
      enableMediaQueryOrder: false,
      styleResolution: "application-order",
      propertyValidationMode: "throw",
    }]],
  });
  if (!compiled?.code) throw new Error(`StyleX produced no module for ${filename}`);
  const javascript = new Bun.Transpiler({ loader: "ts" }).transformSync(compiled.code);
  const executable = javascript
    .replace(/^import\s+\*\s+as\s+stylex\s+from\s+["']@stylexjs\/stylex["'];?\s*$/gm, "")
    .replace(/^export\s+(?=(?:const|let|var|function)\b)/gm, "");
  if (/^\s*(?:import|export)\b/m.test(executable)) {
    throw new Error(`StyleX precompiler only accepts self-contained attribute modules: ${filename}`);
  }
  const attributes = new Function("stylex", `${executable}\nreturn attributes;`)(dependencies.stylexRuntime);
  if (!attributes || typeof attributes !== "object") throw new Error(`${filename} must export an attributes map`);
  for (const [name, attrs] of Object.entries(attributes)) {
    if (!attrs || typeof attrs !== "object" || Array.isArray(attrs)) throw new Error(`Invalid attributes: ${name}`);
    for (const [key, value] of Object.entries(attrs)) {
      if (key !== "class" || typeof value !== "string") throw new Error(`Static StyleX attribute ${name}.${key} is not serializable`);
    }
  }
  const rules = (compiled.metadata?.stylex ?? []) as unknown[];
  return {
    attributes,
    css: dependencies.processStylexRules(rules, false),
    sourceHash: createHash("sha256").update(source).digest("hex"),
  };
}
