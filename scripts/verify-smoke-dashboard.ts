#!/usr/bin/env bun
// Verifies the dashboard HTML produced by `rentemester dashboard` during the
// smoke run is non-trivial and well-formed enough to render in a browser.

import { existsSync, statSync, readFileSync } from "node:fs";

const path = process.argv[2];
if (!path) {
  console.error("Usage: verify-smoke-dashboard.ts <dashboard.html>");
  process.exit(2);
}

if (!existsSync(path)) {
  console.error(`dashboard verification failed: ${path} does not exist`);
  process.exit(1);
}

const stat = statSync(path);
if (stat.size < 1024) {
  console.error(`dashboard verification failed: ${path} is only ${stat.size} bytes (<1KB)`);
  process.exit(1);
}

const html = readFileSync(path, "utf8");
const checks: Array<[string, boolean]> = [
  ["doctype", html.startsWith("<!DOCTYPE html>")],
  ["html_lang_da", /<html\b[^>]*\blang="da"[^>]*>/.test(html)],
  ["body_open", /<body\b[^>]*>/.test(html)],
  ["body_close", html.includes("</body>")],
  ["html_close", html.trimEnd().endsWith("</html>")],
  ["has_header", /<header\b[^>]*>/.test(html)],
  ["has_metrics", /<section\b[^>]*>/.test(html)],
  ["has_footer", /<footer\b[^>]*>/.test(html)],
  ["offline_fonts", html.includes("new FontFace") && !html.includes("https://fonts.")],
];

const failed = checks.filter(([, ok]) => !ok);
if (failed.length > 0) {
  for (const [name] of failed) console.error(`dashboard verification failed: ${name}`);
  process.exit(1);
}

console.log(`dashboard verification ok: ${path} (${stat.size} bytes)`);
