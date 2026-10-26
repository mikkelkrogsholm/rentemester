import { readdir, readFile } from "node:fs/promises";
import { relative, resolve } from "node:path";
import ts from "typescript";

export type StylingViolation = { path: string; line: number; reason: string };
const roots = ["app/src", "app/scripts", "www/src", "www/scripts", "www/public", "src", "scripts"];
const entryFiles = ["app/index.html", "app/gallery.html"];
// These are compiler products, checked by their reproducible generators. This
// list deliberately names files rather than permitting arbitrary generated CSS.
const generated = new Set(["src/design/document-styles.generated.ts", "src/design/document-fonts.generated.ts", "app/src/design/favicon.generated.svg", "www/public/favicon.svg", "www/public/og-default.svg", "www/public/og-hvorfor.svg", "www/public/og-funktioner.svg", "www/public/og-saadan-virker-det.svg", "www/public/og-installation.svg"]);
const compiledCssSlots: Record<string, Set<string>> = {
  "src/core/backup-guide.ts": new Set(["documentCss"]),
  "src/core/compliance-report.ts": new Set(["documentCss"]),
  "src/core/ixbrl.ts": new Set(["escapeXml(documentCss)"]),
  "src/core/dashboard/page.ts": new Set(["buildStyle()"]),
  "www/scripts/svg-templates.ts": new Set(["css"]),
  "scripts/document-styles.ts": new Set(["css"]),
};
const assetAttributes = new Set(["fill", "stroke", "stroke-width", "strokeWidth", "stop-color", "stopColor", "stop-opacity", "stopOpacity", "font-family", "fontFamily", "font-size", "fontSize", "font-weight", "fontWeight", "text-anchor", "textAnchor"]);

export function inspectStyling(path: string, source: string): StylingViolation[] {
  const violations: StylingViolation[] = [];
  const add = (position: number, reason: string) => {
    const line = source.slice(0, position).split("\n").length;
    if (!violations.some((item) => item.line === line && item.reason === reason)) violations.push({ path, line, reason });
  };
  if (/\.(?:css|scss|sass|less)$/.test(path)) {
    add(0, "hand-authored stylesheet; define styles in StyleX");
    return violations;
  }
  const ast = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true, path.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const literalTexts: Array<{ text: string; position: number }> = [];
  const visit = (node: ts.Node) => {
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) || ts.isTemplateExpression(node)) literalTexts.push({ text: node.getText(ast).slice(1, -1), position: node.getStart(ast) + 1 });
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier) && /\.(?:css|scss|sass|less)(?:\?|$)/.test(node.moduleSpecifier.text)) add(node.getStart(ast), "stylesheet import outside StyleX compiler");
    if (ts.isJsxAttribute(node)) {
      const name = node.name.getText(ast);
      if (name === "style") add(node.getStart(ast), "hand-authored JSX style; spread StyleX output");
      if ((name === "className" || name === "class") && node.initializer && ts.isStringLiteral(node.initializer)) add(node.getStart(ast), "literal styling class; compose StyleX attributes");
      if ((name === "className" || name === "class") && node.initializer && ts.isJsxExpression(node.initializer)) {
        const literalClasses = (child: ts.Node) => {
          if ((ts.isStringLiteral(child) || ts.isNoSubstitutionTemplateLiteral(child) || ts.isTemplateHead(child) || ts.isTemplateMiddle(child) || ts.isTemplateTail(child)) && child.text.trim() && !ts.isElementAccessExpression(child.parent)) add(child.getStart(ast), "conditional literal styling class; compose StyleX variants");
          ts.forEachChild(child, literalClasses);
        };
        literalClasses(node.initializer);
      }
      if (assetAttributes.has(name)) add(node.getStart(ast), "SVG presentation attribute; define visual styling in StyleX");
    }
    if (ts.isBinaryExpression(node) && node.operatorToken.kind >= ts.SyntaxKind.FirstAssignment && node.operatorToken.kind <= ts.SyntaxKind.LastAssignment && /(?:\.style(?:\.|\[)|\.cssText\b)/.test(node.left.getText(ast))) add(node.getStart(ast), "imperative style mutation outside StyleX");
    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsToken && /\.className$/.test(node.left.getText(ast)) && ts.isStringLiteral(node.right) && node.right.text) add(node.getStart(ast), "literal class mutation outside StyleX");
    if (ts.isCallExpression(node)) {
      if (/\.style\.(?:setProperty|removeProperty)$/.test(node.expression.getText(ast))) add(node.getStart(ast), "imperative style mutation outside StyleX");
      if (/\.classList\.(?:add|remove|toggle|replace)$/.test(node.expression.getText(ast)) && node.arguments.some((argument) => ts.isStringLiteral(argument) && argument.text.length > 0)) add(node.getStart(ast), "literal class mutation outside StyleX");
      if (/\.(?:insertRule|addRule|replaceSync)$/.test(node.expression.getText(ast))) add(node.getStart(ast), "manual stylesheet injection");
      if (/\.setAttribute$/.test(node.expression.getText(ast)) && node.arguments[0] && ts.isStringLiteral(node.arguments[0]) && node.arguments[0].text === "style") add(node.getStart(ast), "manual style attribute mutation");
    }
    ts.forEachChild(node, visit);
  };
  if (!path.endsWith(".astro")) visit(ast);
  if (path.endsWith(".astro")) {
    for (const match of source.matchAll(/\b(?:class|className)\s*=\s*\{([^}]+)\}/g)) {
      if (/["'`][^"'`]+["'`]/.test(match[1]!)) add(match.index!, "literal Astro expression styling class");
    }
  }
  // Astro and string-rendered documents are checked in their source form too.
  for (const match of source.matchAll(/\b(?:class|className)\s*=\s*["']([^"']*)["']/g)) {
    // Takumi's page counters are protocol markers, not styling classes. They
    // have no CSS rules and are confined to the owned renderer adapter.
    if (path === "src/design/pdfcn/components.ts" && /^(?:pageNumber|totalPages)$/.test(match[1]!)) continue;
    if (match[1] && match[1].replace(/\$\{[^}]+\}/g, "").trim()) add(match.index!, "literal markup styling class");
  }
  for (const match of source.matchAll(/\bstyle\s*=\s*(?:["']|\{)/g)) add(match.index!, "hand-authored markup style");
  for (const match of source.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/g)) {
    const interpolation = /^\s*\$\{([^}]+)\}\s*$/.exec(match[1]!);
    if (!interpolation || !compiledCssSlots[path]?.has(interpolation[1]!.trim())) add(match.index!, "style block outside a registered StyleX compiler consumer");
  }
  for (const match of source.matchAll(/<link\b[^>]*>/g)) {
    if (path === "app/scripts/build.ts" && match[0].includes('href="/${cssName}"')) continue;
    if (/rel=["']stylesheet["']/.test(match[0]) && /href=["'](?!\$\{)[^"']+["']/.test(match[0])) add(match.index!, "literal stylesheet link outside compiled StyleX output");
  }
  const cssTexts = path.endsWith(".astro")
    ? [{ text: source.replace(/^---[\s\S]*?---/, (frontmatter) => " ".repeat(frontmatter.length)), position: 0 }]
    : literalTexts;
  for (const literal of cssTexts) {
    for (const match of literal.text.matchAll(/(?:^|[}\n])\s*(?:[.#][\w-]+|[a-z][\w-]*)(?:[\s>+~:[\].#\w()-]*)\{\s*(?:color|background(?:-[a-z]+)?|border(?:-[a-z]+)?|font(?:-[a-z]+)?|display|position|width|height|padding(?:-[a-z]+)?|margin(?:-[a-z]+)?|opacity|fill|stroke|content|overflow(?:-[a-z]+)?|transform|transition|animation|line-height|text-[a-z]+|gap|outline(?:-[a-z]+)?)\s*:[^{}]+;?\s*\}/g)) add(literal.position + match.index!, "raw CSS rules outside StyleX");
  }
  for (const match of source.matchAll(/\b(?:fill|stroke|stroke-width|stop-color|stop-opacity|font-family|font-size|font-weight|text-anchor)\s*=\s*["']/g)) add(match.index!, "SVG presentation attribute; define visual styling in StyleX");
  return violations;
}

async function files(directory: string): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true }).catch(() => []);
  const lists = await Promise.all(entries.filter((entry) => !["node_modules"].includes(entry.name)).map(async (entry) => {
    const path = resolve(directory, entry.name);
    if (path === resolve(import.meta.dir, "../www/.stylex") || path === resolve(import.meta.dir, "../www/public/_stylex")) return [];
    return entry.isDirectory() ? files(path) : /\.(?:ts|tsx|astro|html|svg|css|scss|sass|less)$/.test(entry.name) && !/\.(?:test|spec)\./.test(entry.name) ? [path] : [];
  }));
  return lists.flat();
}

export async function checkStylex(repoRoot: string): Promise<StylingViolation[]> {
  const paths = [...(await Promise.all(roots.map((path) => files(resolve(repoRoot, path))))).flat(), ...entryFiles.map(path => resolve(repoRoot, path))];
  const violations = await Promise.all(paths.map(async (absolute) => {
    const path = relative(repoRoot, absolute);
    return generated.has(path) || path === "scripts/check-stylex.ts" ? [] : inspectStyling(path, await readFile(absolute, "utf8"));
  }));
  return violations.flat().sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line);
}

if (import.meta.main) {
  const violations = await checkStylex(resolve(import.meta.dir, ".."));
  for (const item of violations) console.error(`${item.path}:${item.line}: ${item.reason}`);
  if (violations.length) {
    console.error(`StyleX ownership check failed: ${violations.length} violations`);
    process.exitCode = 1;
  } else console.log("StyleX ownership verified across cockpit, site, documents and generators");
}
