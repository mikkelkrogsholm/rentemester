import { documentAttr } from "../../design/document-html";
// Page footer — generation timestamp + tucked-away technical provenance.

import { escapeHtml, formatTimestampShort, type DashboardInput } from "./_shared";

export function footer(input: DashboardInput): string {
  const generated = formatTimestampShort(input.generatedAt);
  // The footer faces the owner, not a developer. The raw commit hash and the
  // long rule-bundle-version string are build provenance — kept for support
  // traceability but tucked into a small <details>, never dumped on the calm
  // cockpit surface. The visible line is just "genereret <tid>". (#246)
  const provenance =
    `<details ${documentAttr("provenance")}><summary ${documentAttr("summary")}>Teknisk version</summary>` +
    `<span ${documentAttr("mono")}>commit ${escapeHtml(input.commitSha)}</span> · ` +
    `<span ${documentAttr("mono")}>regelsæt ${escapeHtml(input.ruleBundleVersion)}</span></details>`;
  return `<footer ${documentAttr("footer")}>
  <div ${documentAttr("row")}>
    <div>Genereret <span ${documentAttr("mono")}>${escapeHtml(generated)}</span> · Rentemester</div>
    <div ${documentAttr("mono")}>github.com/mikkelkrogsholm/rentemester</div>
  </div>
  ${provenance}
</footer>`;
}
