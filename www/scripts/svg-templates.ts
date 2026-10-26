type SvgAttributes = Record<string, Record<string, string>>;
const attrs = (value: Record<string, string>) => Object.entries(value).map(([key, value]) => `${key}="${escapeXml(value)}"`).join(" ");

interface Variant {
  slug: string;
  eyebrow: string;
  title: string[];
  subtitle: string;
}

const variants: readonly Variant[] = [
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

function buildSvg(v: Variant, sx: SvgAttributes, css: string): string {
  const line1 = escapeXml(v.title[0]);
  const line2 = escapeXml(v.title[1] ?? "");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 630" width="1200" height="630">
  <style data-stylex="compiled">${css}</style>
  <defs>
    <radialGradient id="glow" cx="50%" cy="40%" r="60%">
      <stop offset="0%" ${attrs(sx.glowStart)}/>
      <stop offset="100%" ${attrs(sx.glowEnd)}/>
    </radialGradient>
    <radialGradient id="cyber" cx="80%" cy="80%" r="30%">
      <stop offset="0%" ${attrs(sx.cyberStart)}/>
      <stop offset="100%" ${attrs(sx.cyberEnd)}/>
    </radialGradient>
  </defs>
  <rect width="1200" height="630" ${attrs(sx.background)}/>
  <rect width="1200" height="630" ${attrs(sx.glowLayer)}/>
  <rect width="1200" height="630" ${attrs(sx.cyberLayer)}/>
  <text x="80" y="120" ${attrs(sx.eyebrow)}>${escapeXml(v.eyebrow)}</text>
  <text x="80" y="290" ${attrs(sx.title)}>${line1}</text>
  ${line2 ? `<text x="80" y="400" ${attrs(sx.titleAccent)}>${line2}</text>` : ""}
  <line x1="80" y1="450" x2="600" y2="450" ${attrs(sx.line)}/>
  <text x="80" y="510" ${attrs(sx.subtitle)}>${escapeXml(v.subtitle)}</text>
  <text x="80" y="570" ${attrs(sx.footer)}>rentemester.dk · github.com/mikkelkrogsholm/rentemester</text>
  <rect x="1060" y="80" width="60" height="60" ${attrs(sx.monogramFrame)}/>
  <text x="1090" y="118" ${attrs(sx.monogram)}>R</text>
</svg>`;
}

export const ogImageNames = variants.map(variant => variant.slug);

/** Pure templates consume the same compiled generation as the website. */
export function buildWebsiteSvgArtifacts(sx: SvgAttributes, css: string): Record<string, string> {
  const artifacts = Object.fromEntries(variants.map(variant => [variant.slug + ".svg", buildSvg(variant, sx, css)]));
  artifacts["favicon.svg"] = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
<style data-stylex="compiled">${css}</style>
<rect width="32" height="32" ${attrs(sx.background)}/>
<text x="16" y="23" ${attrs(sx.faviconText)}>R</text>
<rect x="4" y="27" width="24" height="1" ${attrs(sx.faviconLine)}/>
</svg>
`;
  return artifacts;
}
