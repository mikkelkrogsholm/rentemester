import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const source = await readFile(resolve(root, "DESIGN.md"), "utf8");
const front = source.match(/^---\n([\s\S]*?)\n---/);
if (!front) throw new Error("DESIGN.md: missing YAML frontmatter");
type TokenTree = { [key: string]: string | TokenTree };
const tree = Bun.YAML.parse(front[1]!) as TokenTree;
const camel = (key: string) =>
	key.replace(/-([a-z0-9])/g, (_, letter: string) => letter.toUpperCase());
function normalized(node: TokenTree): TokenTree {
	return Object.fromEntries(
		Object.entries(node).map(([key, value]) => [
			camel(key),
			typeof value === "object" ? normalized(value) : value,
		]),
	);
}
const tokens = normalized(tree);
function resolveValue(value: string, trail: string[] = []): string {
	const reference = value.match(/^\{([^}]+)\}$/)?.[1];
	if (!reference) return value;
	if (trail.includes(reference)) throw new Error(`Cyclic token: ${reference}`);
	const found = reference
		.split(".")
		.reduce<string | TokenTree | undefined>(
			(node, part) =>
				typeof node === "object" ? node[camel(part)] : undefined,
			tokens,
		);
	if (typeof found !== "string") throw new Error(`Unknown token: ${reference}`);
	return resolveValue(found, [...trail, reference]);
}
function resolved(node: TokenTree): TokenTree {
	return Object.fromEntries(
		Object.entries(node).map(([key, value]) => [
			key,
			typeof value === "object" ? resolved(value) : resolveValue(value),
		]),
	);
}
const all = resolved(tokens);
const colors = all.colors as Record<string, string>;
const typography = all.typography as Record<string, string>;
const spacing = all.spacing as Record<string, string>;
const rounded = all.rounded as Record<string, string>;
const groups = [
	"colors",
	"typography",
	"spacing",
	"rounded",
	"layout",
	"controls",
] as const;
const header =
	"// Generated from DESIGN.md by scripts/design-tokens.ts. Do not edit.\n";
const raw =
	header +
	`export const designTokens = ${JSON.stringify(all, null, 2)} as const;\n`;
// CSS family names containing a numeric word (Source Serif 4) require quotes.
// Raw chart/document tokens retain the plain name; CSS tokens include fallbacks.
const cssTypography = Object.fromEntries(
	Object.entries(typography).map(([key, value]) => [
		key,
		key.endsWith("Family")
			? `${JSON.stringify(value)}, ${key === "headlineFamily" ? "Georgia, serif" : key === "monoFamily" ? "monospace" : "sans-serif"}`
			: value,
	]),
);
const stylex =
	header +
	`import * as stylex from '@stylexjs/stylex';\n` +
	groups
		.map(
			(group) =>
				`export const ${group} = stylex.defineVars(${JSON.stringify(group === "typography" ? cssTypography : all[group], null, 2)});\n`,
		)
		.join("");
const outputs = new Map([
	["src/design/tokens.ts", raw],
	["app/src/design/tokens.stylex.ts", stylex],
]);
for (const [path, output] of outputs) {
	const absolute = resolve(root, path);
	if (Bun.argv.includes("--check")) {
		if ((await readFile(absolute, "utf8").catch(() => "")) !== output)
			throw new Error(`Stale design tokens: ${path}`);
	} else {
		await mkdir(resolve(absolute, ".."), { recursive: true });
		await writeFile(absolute, output);
	}
}
