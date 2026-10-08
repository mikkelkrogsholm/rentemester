import { documentAttr, documentCss, documentFontScript } from "../design/document-html";
// Compliance report: a deterministic, self-contained HTML document a business
// owner can hand to an auditor or revisor. It assembles audit-chain status,
// backup compliance, retention deadlines, GDPR posture, regulatory coverage
// and the cited rules-to-statute map into one printable page.
//
// Render contract: byte-for-byte deterministic. The caller (CLI/MCP) collects
// every piece of state and passes it as input — this module reads nothing
// from the filesystem, the database or the clock.

import { createHash } from "node:crypto";
import type { BackupGovernanceStatus } from "./backup-governance";
import type { RetentionStatusReport } from "./retention";
import type { RegulatoryCoverage } from "./regulatory-coverage";
import type { RuleMetadata } from "./rules-metadata";

export type ComplianceReportInput = {
  generatedAt: string; // ISO 8601 UTC, supplied by the caller
  companyName: string;
  companyCvr: string | null;
  fiscalYearLabel: string | null;
  commitSha: string | null;
  ruleBundleVersion: string;
  audit: {
    ok: boolean;
    entryCount: number;
    errors: string[];
  };
  backup: BackupGovernanceStatus;
  retention: RetentionStatusReport;
  periods: {
    closedCount: number;
    lastClosedLabel: string | null;
  };
  gdpr: {
    eventCount: number;
    fingerprint: string;
  };
  coverage: RegulatoryCoverage;
  rules: RuleMetadata[];
};



function escapeHtml(value: string | number | null | undefined): string {
  if (value == null) return "";
  const str = String(value);
  let out = "";
  for (let i = 0; i < str.length; i += 1) {
    switch (str.charCodeAt(i)) {
      case 38: out += "&amp;"; break;
      case 60: out += "&lt;"; break;
      case 62: out += "&gt;"; break;
      case 34: out += "&quot;"; break;
      case 39: out += "&#39;"; break;
      default: out += str[i];
    }
  }
  return out;
}

function formatInstant(iso: string | null): string {
  if (!iso) return "—";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return escapeHtml(iso);
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    `${date.getUTCFullYear()}-${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())} ` +
    `${pad(date.getUTCHours())}:${pad(date.getUTCMinutes())} UTC`
  );
}

function pill(kind: "ok" | "warn" | "bad", text: string): string {
  return `<span ${documentAttr(kind === "ok" ? "pillOk" : kind === "warn" ? "pillWarn" : "pillBad")}>${escapeHtml(text)}</span>`;
}

function auditSection(input: ComplianceReportInput): string {
  const auditPill = input.audit.ok
    ? pill("ok", "Verificeret")
    : pill("bad", "Brudt kæde");
  const errors = input.audit.errors.length === 0
    ? ""
    : `<p ${documentAttr("law")}>Fejl: ${input.audit.errors.map(escapeHtml).join("; ")}</p>`;
  return `<section ${documentAttr("card")}>
<h2 ${documentAttr("guideH2")}>1. Integritet af bogføringen</h2>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Audit-kæde verificeret</span><span ${documentAttr("metricValue")}>${auditPill}</span></div>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Antal posterede entries</span><span ${documentAttr("metricValue")}>${input.audit.entryCount}</span></div>
${errors}
<p ${documentAttr("muted")}>Rentemester verificerer hash-kæden over alle posterede bogføringsposter ved at replaye SHA-256 over hver posts kanoniske JSON. Hver post bærer en reference til sin forgænger; en mutation af en gammel post brydes mod den næste hash. Rettelser bogføres som en ny linket modpostering (<code ${documentAttr("code")}>reversal_of_entry_id</code>) — originalen ændres aldrig.</p>
<p ${documentAttr("law")}><strong>Hjemmel:</strong> Bogføringsloven § 13 stk. 1 (sikring mod ødelæggelse, fejl og misbrug) og § 9 stk. 3 (rettelser skal vises tydeligt). Verificeres af <code ${documentAttr("code")}>rentemester audit verify</code>.</p>
</section>`;
}

function backupSection(input: ComplianceReportInput): string {
  const b = input.backup;
  const overallPill = b.hasCompliantDestination
    ? pill("ok", "Opfyldt")
    : pill("bad", "Ikke opfyldt");
  const offsitePill = b.latestBackupPlacedOffsite
    ? pill("ok", "Ja")
    : pill("warn", "Nej");
  const lastBackup = b.compliance.latestBackupId ?? "—";
  const destinations = b.destinations
    .map((d) => {
      const region = d.regionAttestation.inEeaOrEu
        ? "✔ " + escapeHtml(d.regionAttestation.country ?? "")
        : "—";
      const security = d.itSecurityAttestation?.meetsRecognisedStandards ? "✔" : "—";
      return `<tr><td ${documentAttr("guideCell")}>${escapeHtml(d.label)}</td><td ${documentAttr("guideCell")}>${escapeHtml(d.kind)}</td>` +
        `<td ${documentAttr("guideCell")}>${region}</td><td ${documentAttr("guideCell")}>${security}</td></tr>`;
    })
    .join("\n");
  const destTable = destinations.length === 0
    ? `<p ${documentAttr("muted")}>Ingen destinationer registreret.</p>`
    : `<div ${documentAttr("tableScroll")} tabindex="0" role="region" aria-label="Regnskabstabel"><table ${documentAttr("guideTable")}><thead><tr><th ${documentAttr("guideTh")}>Label</th><th ${documentAttr("guideTh")}>Type</th><th ${documentAttr("guideTh")}>EU/EØS</th><th ${documentAttr("guideTh")}>IT-sikkerhed</th></tr></thead><tbody>${destinations}</tbody></table></div>`;
  return `<section ${documentAttr("card")}>
<h2 ${documentAttr("guideH2")}>2. Opbevaring og backup</h2>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Backup-pligt opfyldt</span><span ${documentAttr("metricValue")}>${overallPill}</span></div>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Seneste backup-ID</span><span ${documentAttr("metricValue")}>${escapeHtml(lastBackup)}</span></div>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Senest placeret offsite</span><span ${documentAttr("metricValue")}>${offsitePill}</span></div>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Antal destinationer</span><span ${documentAttr("metricValue")}>${b.destinationCount}</span></div>
${destTable}
<p ${documentAttr("muted")}>Backups signeres med HMAC-SHA256 (default) eller Ed25519 (opt-in). Den offentlige nøgle pakkes ind i backup-arkivet så en revisor kan verificere uden adgang til den private nøgle.</p>
<p ${documentAttr("law")}><strong>Hjemmel:</strong> Bogføringsloven § 12 stk. 1 (5-årig betryggende opbevaring) og BEK 205 § 4 stk. 1-2 (ugentlig fuld sikkerhedskopi hos ikke-nærtstående tredjepart på EU/EØS-server). Vejledning: <code ${documentAttr("code")}>rentemester system backup-guide</code>.</p>
</section>`;
}

function retentionSection(input: ComplianceReportInput): string {
  const r = input.retention;
  const overallExpired = r.rows.reduce((sum, row) => sum + row.expired, 0);
  const overallPill = overallExpired === 0
    ? pill("ok", "Inden for fristen")
    : pill("warn", `${overallExpired} udløbet`);
  const labelOf = (table: string) =>
    table === "documents" ? "Bilag"
    : table === "journal_entries" ? "Bogføringsposter"
    : "Banktransaktioner";
  const rows = r.rows
    .map((row) =>
      `<tr><td ${documentAttr("guideCell")}>${escapeHtml(labelOf(row.table))}</td><td ${documentAttr("guideCell")}>${row.total}</td>` +
      `<td ${documentAttr("guideCell")}>${row.expired}</td><td ${documentAttr("guideCell")}>${escapeHtml(row.nextExpiry ?? "—")}</td>` +
      `<td ${documentAttr("guideCell")}>${escapeHtml(row.oldestExpired ?? "—")}</td></tr>`
    )
    .join("\n");
  return `<section ${documentAttr("card")}>
<h2 ${documentAttr("guideH2")}>3. Opbevaringsfrist (5 år)</h2>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Status pr. ${escapeHtml(r.asOf)}</span><span ${documentAttr("metricValue")}>${overallPill}</span></div>
<div ${documentAttr("tableScroll")} tabindex="0" role="region" aria-label="Regnskabstabel"><table ${documentAttr("guideTable")}>
<thead><tr><th ${documentAttr("guideTh")}>Materiale</th><th ${documentAttr("guideTh")}>Total</th><th ${documentAttr("guideTh")}>Udløbet</th><th ${documentAttr("guideTh")}>Næste udløb</th><th ${documentAttr("guideTh")}>Ældste udløbet</th></tr></thead>
<tbody>${rows}</tbody>
</table></div>
<p ${documentAttr("muted")}>Retain-until beregnes som udgangen af regnskabsåret plus 5 kalenderår. Bogføringsposter, dokumenter og banktransaktioner får hver deres deadline. Materiale inden for fristen kan ikke slettes — heller ikke ved en GDPR-anmodning (§ 13 stk. 1).</p>
<p ${documentAttr("law")}><strong>Hjemmel:</strong> Bogføringsloven § 12 stk. 1. Liste: <code ${documentAttr("code")}>rentemester retention status</code>.</p>
</section>`;
}

function periodSection(input: ComplianceReportInput): string {
  return `<section ${documentAttr("card")}>
<h2 ${documentAttr("guideH2")}>4. Periode-lukning</h2>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Lukkede regnskabsperioder</span><span ${documentAttr("metricValue")}>${input.periods.closedCount}</span></div>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Senest lukkede periode</span><span ${documentAttr("metricValue")}>${escapeHtml(input.periods.lastClosedLabel ?? "—")}</span></div>
<p ${documentAttr("muted")}>En lukket periode kan ikke modtage nye eller ændrede posteringer. Genåbning bogføres som en audit-event (immutable) med eksplicit grund — selve lukke-rækken muteres aldrig. Periodens effektive tilstand replayes fra audit-historikken hvert gang den evalueres.</p>
<p ${documentAttr("law")}><strong>Hjemmel:</strong> BEK 205 § 3 stk. 3. Implementering: <code ${documentAttr("code")}>rentemester period close</code> / <code ${documentAttr("code")}>period reopen</code>.</p>
</section>`;
}

function gdprSection(input: ComplianceReportInput): string {
  return `<section ${documentAttr("card")}>
<h2 ${documentAttr("guideH2")}>5. GDPR — persondata</h2>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>GDPR-events i audit-log</span><span ${documentAttr("metricValue")}>${input.gdpr.eventCount}</span></div>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Audit-fingerprint</span><span ${documentAttr("metricValue")}>${escapeHtml(input.gdpr.fingerprint)}</span></div>
<p ${documentAttr("muted")}>Indsigt (art. 15) leveres som en signed JSON-pakke via <code ${documentAttr("code")}>rentemester gdpr export</code>. Sletning (art. 17) gemmes som en append-only tombstone i <code ${documentAttr("code")}>gdpr_erasures</code>; bogføringspligtige felter afvises indtil retention-fristen er udløbet (GDPR art. 17 stk. 3 lit. b — retlig forpligtelse).</p>
<p ${documentAttr("law")}><strong>Hjemmel:</strong> GDPR (EU 2016/679) art. 15 og 17. Audit: <code ${documentAttr("code")}>rentemester gdpr audit-log</code>.</p>
</section>`;
}

function coverageSection(input: ComplianceReportInput): string {
  const c = input.coverage;
  const pct = c.overall.inScopeOperativeCount === 0
    ? "—"
    : Math.round((c.overall.inScopeCitedCount * 100) / c.overall.inScopeOperativeCount) + "%";
  const allClean = c.closureErrors.length === 0 && c.driftErrors.length === 0 && c.scopeErrors.length === 0;
  const integrityPill = allClean ? pill("ok", "Ingen fejl") : pill("bad", "Fejl");
  const perSourceRows = c.perSource
    .filter((s) => s.inScopeOperativeCount > 0)
    .map(
      (s) =>
        `<tr><td ${documentAttr("guideCell")}>${escapeHtml(s.sourceId)}</td>` +
        `<td ${documentAttr("guideCell")}>${s.inScopeCitedCount}/${s.inScopeOperativeCount}</td></tr>`,
    )
    .join("\n");
  return `<section ${documentAttr("card")}>
<h2 ${documentAttr("guideH2")}>6. Regulatorisk dækning</h2>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>In-scope dækning</span><span ${documentAttr("metricValue")}>${c.overall.inScopeCitedCount}/${c.overall.inScopeOperativeCount} (${pct})</span></div>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Closure / drift / scope errors</span><span ${documentAttr("metricValue")}>${integrityPill} (${c.closureErrors.length}/${c.driftErrors.length}/${c.scopeErrors.length})</span></div>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Uncited regler (allowlisted)</span><span ${documentAttr("metricValue")}>${c.uncitedRules.length}</span></div>
<p ${documentAttr("muted")}>Tallet måler hvor stor en del af de in-scope danske lovbestemmelser der citeres af en eksekverbar regel. Tælleren stiger når en ny paragraf cites; nævneren er den scope-erklærede delmængde af lovkorpusset (se <code ${documentAttr("code")}>sources/scope.yaml</code>).</p>
<div ${documentAttr("tableScroll")} tabindex="0" role="region" aria-label="Regnskabstabel"><table ${documentAttr("guideTable")}>
<thead><tr><th ${documentAttr("guideTh")}>Kilde</th><th ${documentAttr("guideTh")}>Citeret / In-scope</th></tr></thead>
<tbody>${perSourceRows}</tbody>
</table></div>
<p ${documentAttr("muted")}>Kommando: <code ${documentAttr("code")}>rentemester reg coverage</code>. Verbatim citation-review: <code ${documentAttr("code")}>rentemester reg citations</code>.</p>
</section>`;
}

function rulesByCategory(rules: RuleMetadata[]): Map<string, RuleMetadata[]> {
  const groups = new Map<string, RuleMetadata[]>();
  for (const rule of rules) {
    const key = rule.category;
    let bucket = groups.get(key);
    if (!bucket) {
      bucket = [];
      groups.set(key, bucket);
    }
    bucket.push(rule);
  }
  for (const bucket of groups.values()) {
    bucket.sort((a, b) => (a.ruleId < b.ruleId ? -1 : a.ruleId > b.ruleId ? 1 : 0));
  }
  return groups;
}

function citationsSection(input: ComplianceReportInput): string {
  const groups = rulesByCategory(input.rules);
  const orderedCategories = [...groups.keys()].sort();
  const blocks: string[] = [];
  for (const category of orderedCategories) {
    const ruleRows = (groups.get(category) ?? [])
      .map((rule) => {
        const cites = rule.provisions.length === 0
          ? `<em>ingen citation — se allowlist</em>`
          : rule.provisions
              .map((p) => `${escapeHtml(rule.sourceId)} ${escapeHtml(p.ref)}`)
              .join("<br>");
        return `<tr><td ${documentAttr("guideCell")}><code ${documentAttr("code")}>${escapeHtml(rule.ruleId)}</code><br><span ${documentAttr("sub")}>${escapeHtml(rule.name)}</span></td><td ${documentAttr("guideCell")}>${cites}</td></tr>`;
      })
      .join("\n");
    blocks.push(
      `<h3 ${documentAttr("guideH3")}>${escapeHtml(category)}</h3>` +
      `<div ${documentAttr("tableScroll")} tabindex="0" role="region" aria-label="Regnskabstabel"><table ${documentAttr("guideTable")}><thead><tr><th ${documentAttr("guideTh")}>Regel</th><th ${documentAttr("guideTh")}>Citation</th></tr></thead><tbody>${ruleRows}</tbody></table></div>`,
    );
  }
  return `<section ${documentAttr("card")}>
<h2 ${documentAttr("guideH2")}>7. Regler og deres lovhjemmel</h2>
<p ${documentAttr("muted")}>For hver implementeret bogføringsregel vises ID, beskrivelse og den paragraf den citerer. En revisor kan opklare det forretningsmæssige spørgsmål "hvor i loven står det?" ved at slå reglens ID op her.</p>
${blocks.join("\n")}
</section>`;
}

function authoritySection(): string {
  return `<section ${documentAttr("card")}>
<h2 ${documentAttr("guideH2")}>8. Myndighedsudlevering og SAF-T</h2>
<p ${documentAttr("muted")}>Rentemester kan på anmodning levere bogføringsmaterialet til en myndighed (Skattestyrelsen, Erhvervsstyrelsen) som en deterministisk eksport-pakke med SHA-256-manifest. Frist: 4 uger fra anmodningstidspunktet, jf. BEK 97 § 11 stk. 1.</p>
<ul ${documentAttr("ul")}>
<li ${documentAttr("li")}><code ${documentAttr("code")}>rentemester system export-authority --requested-at &lt;iso&gt; --requester "Skattestyrelsen" --out &lt;dir&gt;</code> — fuld pakke med bogføringsposter, bilag, banktransaktioner, audit-log og manifest.</li>
<li ${documentAttr("li")}><code ${documentAttr("code")}>rentemester system export-saft --from YYYY-MM-DD --to YYYY-MM-DD --out &lt;dir&gt;</code> — SAF-T-eksport (Standard Audit File for Tax) for kontoplan, journal, salgs- og indkøbsfakturaer samt master-files.</li>
<li ${documentAttr("li")}><code ${documentAttr("code")}>rentemester system export-accountant --out &lt;dir&gt;</code> — håndoff-pakke til revisor eller bogholder.</li>
</ul>
<p ${documentAttr("law")}><strong>Hjemmel:</strong> BEK 97 §§ 10-11 (digitale standard bogføringssystemer skal stille materialet til rådighed inden for 4 uger).</p>
</section>`;
}

function reportFingerprint(body: string): string {
  return createHash("sha256").update(body, "utf8").digest("hex");
}

export function renderComplianceReport(input: ComplianceReportInput): string {
  const body =
    auditSection(input) +
    backupSection(input) +
    retentionSection(input) +
    periodSection(input) +
    gdprSection(input) +
    coverageSection(input) +
    citationsSection(input) +
    authoritySection();
  const fingerprint = reportFingerprint(body);
  const cvr = input.companyCvr ? ` (CVR ${escapeHtml(input.companyCvr)})` : "";
  const fiscalYear = input.fiscalYearLabel
    ? `Regnskabsår: ${escapeHtml(input.fiscalYearLabel)} · `
    : "";
  return `<!doctype html>
<html lang="da" ${documentAttr("root")}>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Compliance-rapport — ${escapeHtml(input.companyName)}</title>
<style>${documentCss}</style>
${documentFontScript()}
</head>
<body ${documentAttr("guideBody")}>
<main ${documentAttr("guideMain")}>
<h1 ${documentAttr("guideH1")}>Compliance-rapport</h1>
<p ${documentAttr("sub")}>${escapeHtml(input.companyName)}${cvr} · ${fiscalYear}Genereret ${escapeHtml(formatInstant(input.generatedAt))}</p>

<section ${documentAttr("card")}>
<h2 ${documentAttr("guideH2")}>Forretningsmæssigt overblik</h2>
<p ${documentAttr("muted")}>Denne rapport dokumenterer hvordan Rentemester driver bogføringen i overensstemmelse med dansk lovgivning. Rapporten er en deterministisk funktion af virksomhedens nuværende ledger-tilstand og det regel-bundle der er installeret — samme input giver byte-for-byte identisk output. Hash i bunden tillader en revisor at konstatere at en udleveret kopi ikke er ændret.</p>
<p ${documentAttr("muted")}>Rapporten dækker:</p>
<ul ${documentAttr("ul")}>
<li ${documentAttr("li")}>Integritet af bogføringen (hash-kæde + append-only)</li>
<li ${documentAttr("li")}>Opbevaring og backup (5 års betryggende opbevaring + ugentlig sikkerhedskopi)</li>
<li ${documentAttr("li")}>Periode-lukning og rettelser via reversal</li>
<li ${documentAttr("li")}>GDPR-håndtering og retention-grænse for sletning</li>
<li ${documentAttr("li")}>Regulatorisk dækning (citationer mod lovkorpusset)</li>
<li ${documentAttr("li")}>Liste af regler og deres paragraf-hjemmel</li>
<li ${documentAttr("li")}>Myndighedsudlevering, SAF-T og revisor-eksport</li>
</ul>
</section>

${body}

<footer ${documentAttr("guideFooter")}>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Rule bundle</span><span ${documentAttr("metricValue")}>${escapeHtml(input.ruleBundleVersion)}</span></div>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Commit</span><span ${documentAttr("metricValue")}>${escapeHtml(input.commitSha ?? "—")}</span></div>
<div ${documentAttr("metric")}><span ${documentAttr("muted")}>Rapport-fingerprint</span><span ${documentAttr("metricValue")}>sha256:${fingerprint}</span></div>
<p ${documentAttr("muted")}>Denne rapport er vejledende dokumentation til revisor eller myndighed. Den endelige juridiske vurdering er virksomhedens og dens rådgivers ansvar.</p>
</footer>
</main>
</body>
</html>
`;
}

export function complianceReportFingerprint(html: string): string {
  return createHash("sha256").update(html, "utf8").digest("hex");
}
