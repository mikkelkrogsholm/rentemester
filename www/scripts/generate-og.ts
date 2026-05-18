#!/usr/bin/env bun
import { mkdir, writeFile } from "node:fs/promises";
import { join, dirname } from "node:path";
import { $ } from "bun";

interface Variant {
  slug: string;
  eyebrow: string;
  title: string[];
  subtitle: string;
}

const variants: Variant[] = [
  {
    slug: "og-default",
    eyebrow: "OPEN SOURCE · MIT",
    title: ["Bogholderen", "i maskinen"],
    subtitle: "Open source bogføring til danske virksomheder",
  },
  {
    slug: "og-hvorfor",
    eyebrow: "MANIFEST",
    title: ["Hvorfor", "Rentemester"],
    subtitle: "Agent-first bogføring drevet af åbne danske regler",
  },
  {
    slug: "og-funktioner",
    eyebrow: "FUNKTIONER",
    title: ["Hvad systemet", "kan"],
    subtitle: "Bilag, bank, faktura, moms, audit — sporbart i ledgeren",
  },
  {
    slug: "og-saadan-virker-det",
    eyebrow: "ARKITEKTUR",
    title: ["Sådan", "virker det"],
    subtitle: "Agent · regler · append-only ledger",
  },
  {
    slug: "og-installation",
    eyebrow: "DOKUMENTATION",
    title: ["Installer", "på fem minutter"],
    subtitle: "Bun + git clone + bun install. Det er det.",
  },
];

function escapeXml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

function buildSvg(v: Variant): string {
  const line1 = escapeXml(v.title[0]);
  const line2 = escapeXml(v.title[1] ?? "");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 630" width="1200" height="630">
  <defs>
    <radialGradient id="glow" cx="50%" cy="40%" r="60%">
      <stop offset="0%" stop-color="#e9c176" stop-opacity="0.14"/>
      <stop offset="100%" stop-color="#05070A" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="cyber" cx="80%" cy="80%" r="30%">
      <stop offset="0%" stop-color="#00D1FF" stop-opacity="0.10"/>
      <stop offset="100%" stop-color="#05070A" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="1200" height="630" fill="#05070A"/>
  <rect width="1200" height="630" fill="url(#glow)"/>
  <rect width="1200" height="630" fill="url(#cyber)"/>
  <text x="80" y="120" font-family="JetBrains Mono, monospace" font-size="20" font-weight="600" fill="#00D1FF" letter-spacing="4">${escapeXml(v.eyebrow)}</text>
  <text x="80" y="290" font-family="Georgia, 'EB Garamond', serif" font-size="100" font-weight="600" fill="#e1e2eb">${line1}</text>
  ${line2 ? `<text x="80" y="400" font-family="Georgia, 'EB Garamond', serif" font-size="100" font-weight="600" fill="#e9c176">${line2}</text>` : ""}
  <line x1="80" y1="450" x2="600" y2="450" stroke="#C5A059" stroke-width="1" opacity="0.4"/>
  <text x="80" y="510" font-family="Hanken Grotesk, sans-serif" font-size="28" fill="#d1c5b4">${escapeXml(v.subtitle)}</text>
  <text x="80" y="570" font-family="JetBrains Mono, monospace" font-size="22" fill="#9a8f80">rentemester.dk · github.com/mikkelkrogsholm/rentemester</text>
  <rect x="1060" y="80" width="60" height="60" fill="none" stroke="#C5A059" stroke-width="1" opacity="0.4"/>
  <text x="1090" y="118" font-family="Georgia, 'EB Garamond', serif" font-size="36" font-weight="600" fill="#e9c176" text-anchor="middle">R</text>
</svg>`;
}

const publicDir = join(import.meta.dir, "..", "public");
await mkdir(publicDir, { recursive: true });

for (const v of variants) {
  const svgPath = join(publicDir, `${v.slug}.svg`);
  const pngPath = join(publicDir, `${v.slug}.png`);
  await writeFile(svgPath, buildSvg(v));
  await $`rsvg-convert -w 1200 -h 630 ${svgPath} -o ${pngPath}`.quiet();
  console.log(`✓ ${v.slug}.png`);
}
