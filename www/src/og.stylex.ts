import * as stylex from "@stylexjs/stylex";

// The static compiler supplies this literal from DESIGN.md before evaluation.
declare const marketing: { colors: Record<string, string>; alpha: Record<string, Record<string, string>>; typography: Record<string, string>; layout: Record<string, string> };

const styles = stylex.create({
  faviconText: { fontFamily: marketing.typography.headline, fontSize: 22, fontWeight: 600, textAnchor: "middle", fill: marketing.colors.primary },
  faviconLine: { fill: marketing.colors.brassGold, opacity: 0.6 },
  background: { fill: marketing.colors.inkBlack },
  glowLayer: { fill: "url(#glow)" },
  cyberLayer: { fill: "url(#cyber)" },
  glowStart: { stopColor: marketing.colors.primary, stopOpacity: 0.14 },
  glowEnd: { stopColor: marketing.colors.inkBlack, stopOpacity: 0 },
  cyberStart: { stopColor: marketing.colors.cyberBlue, stopOpacity: 0.1 },
  cyberEnd: { stopColor: marketing.colors.inkBlack, stopOpacity: 0 },
  eyebrow: { fontFamily: marketing.typography.mono, fontSize: 20, fontWeight: 600, fill: marketing.colors.cyberBlue, letterSpacing: 4 },
  title: { fontFamily: marketing.typography.headline, fontSize: 100, fontWeight: 600, fill: marketing.colors.onSurface },
  titleAccent: { fill: marketing.colors.primary },
  line: { stroke: marketing.colors.brassGold, strokeWidth: 1, opacity: 0.4 },
  subtitle: { fontFamily: marketing.typography.body, fontSize: 28, fill: marketing.colors.onSurfaceVariant },
  footer: { fontFamily: marketing.typography.mono, fontSize: 22, fill: marketing.colors.outline },
  monogramFrame: { fill: "none", stroke: marketing.colors.brassGold, strokeWidth: 1, opacity: 0.4 },
  monogram: { fontFamily: marketing.typography.headline, fontSize: 36, fontWeight: 600, fill: marketing.colors.primary, textAnchor: "middle" },
});
export const attributes = {
  faviconText: stylex.attrs(styles.faviconText), faviconLine: stylex.attrs(styles.faviconLine),
  background: stylex.attrs(styles.background), glowLayer: stylex.attrs(styles.glowLayer), cyberLayer: stylex.attrs(styles.cyberLayer),
  glowStart: stylex.attrs(styles.glowStart), glowEnd: stylex.attrs(styles.glowEnd), cyberStart: stylex.attrs(styles.cyberStart), cyberEnd: stylex.attrs(styles.cyberEnd),
  eyebrow: stylex.attrs(styles.eyebrow), title: stylex.attrs(styles.title), titleAccent: stylex.attrs(styles.title, styles.titleAccent),
  line: stylex.attrs(styles.line), subtitle: stylex.attrs(styles.subtitle), footer: stylex.attrs(styles.footer), monogramFrame: stylex.attrs(styles.monogramFrame), monogram: stylex.attrs(styles.monogram),
};
