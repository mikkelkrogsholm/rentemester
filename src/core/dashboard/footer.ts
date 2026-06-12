// Page footer — generation timestamp + tucked-away technical provenance.

import { escapeHtml, formatTimestampShort, type DashboardInput } from "./_shared";

export function footer(input: DashboardInput): string {
  const generated = formatTimestampShort(input.generatedAt);
  // The footer faces the owner, not a developer. The raw commit hash and the
  // long rule-bundle-version string are build provenance — kept for support
  // traceability but tucked into a small <details>, never dumped on the calm
  // cockpit surface. The visible line is just "genereret <tid>". (#246)
  const provenance =
    `<details class="provenance"><summary>Teknisk version</summary>` +
    `<span class="mono">commit ${escapeHtml(input.commitSha)}</span> · ` +
    `<span class="mono">regelsæt ${escapeHtml(input.ruleBundleVersion)}</span></details>`;
  return `<footer class="footer">
  <div class="row">
    <div>Genereret <span class="mono">${escapeHtml(generated)}</span> · Rentemester</div>
    <div class="mono">github.com/mikkelkrogsholm/rentemester</div>
  </div>
  ${provenance}
</footer>`;
}
