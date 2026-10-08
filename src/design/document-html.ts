import { documentAttributes, documentCss } from "./document-styles.generated";
import { embeddedDocumentFonts } from "./document-fonts.generated";

export { documentCss };

/** Only precompiled, composed StyleX attributes enter generated documents. */
export function documentAttr(name: keyof typeof documentAttributes): string {
  return `class="${documentAttributes[name].class}"`;
}

export function escapeDocumentText(value: unknown): string {
  return String(value ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;")
    .replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");
}

/** Offline FontFace registration; font selection stays in compiled StyleX. */
export function documentFontScript(): string {
  return `<script>(function(){if(!("FontFace" in window))return;const fonts=${JSON.stringify(embeddedDocumentFonts)};for(const font of fonts){const options={weight:String(font.weight)};if(font.unicodeRange)options.unicodeRange=font.unicodeRange;const face=new FontFace(font.name,"url(data:font/woff2;base64,"+font.data+")",options);document.fonts.add(face);face.load().catch(function(){});}})();</script>`;
}
