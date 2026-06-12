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

const STYLE = `
:root{--paper:#F4F1EB;--raised:#FBF8F3;--ink:#1B1A17;--muted:#4C4740;--accent:#A6332A;
--danger:#8F2A22;--success:#2E5E4E;--warning:#8A5A12;--line:#D8D2C6;}
*{box-sizing:border-box;}
body{margin:0;background:var(--paper);color:var(--ink);
font-family:"IBM Plex Sans",-apple-system,Segoe UI,Roboto,sans-serif;
font-size:15px;line-height:1.55;}
main{max-width:880px;margin:0 auto;padding:32px 24px 64px;}
h1{font-family:"Source Serif 4",Georgia,serif;font-size:30px;margin:0 0 4px;}
h2{font-family:"Source Serif 4",Georgia,serif;font-size:21px;margin:32px 0 12px;
border-bottom:1px solid var(--line);padding-bottom:6px;}
h3{font-size:15px;margin:18px 0 6px;}
p,li{color:var(--muted);}
.sub{color:var(--muted);font-size:14px;margin:0 0 8px;}
section.card{background:var(--raised);border:1px solid var(--line);border-radius:8px;
padding:16px 20px;margin:16px 0;}
ul{padding-left:20px;}li{margin:4px 0;}
code{font-family:"IBM Plex Mono",monospace;font-size:13px;background:#EFEAE0;
padding:1px 5px;border-radius:3px;color:var(--ink);}
table{width:100%;border-collapse:collapse;margin:8px 0;font-size:13px;}
th,td{text-align:left;padding:6px 8px;border-bottom:1px solid var(--line);vertical-align:top;}
th{color:var(--ink);font-weight:600;font-size:12px;text-transform:uppercase;letter-spacing:0.04em;}
.pill{display:inline-block;padding:2px 10px;border-radius:999px;font-size:13px;
font-weight:600;}
.pill.ok{background:#DCE8E1;color:var(--success);}
.pill.warn{background:#EEE3D1;color:var(--warning);}
.pill.bad{background:#EED9D6;color:var(--danger);}
.metric{display:flex;justify-content:space-between;border-bottom:1px solid var(--line);
padding:6px 0;}
.metric:last-child{border-bottom:none;}
.metric .k{color:var(--muted);}
.metric .v{font-family:"IBM Plex Mono",monospace;color:var(--ink);}
.law{color:var(--muted);font-size:13px;margin-top:8px;border-left:3px solid var(--accent);
padding:4px 10px;background:#F8F2E9;}
footer{margin-top:40px;color:var(--muted);font-size:13px;border-top:1px solid var(--line);
padding-top:16px;}
@media print {body{background:#fff;}main{padding:16px;}section.card{break-inside:avoid;}}
`;

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
  return `<span class="pill ${kind}">${escapeHtml(text)}</span>`;
}

function auditSection(input: ComplianceReportInput): string {
  const auditPill = input.audit.ok
    ? pill("ok", "Verificeret")
    : pill("bad", "Brudt kæde");
  const errors = input.audit.errors.length === 0
    ? ""
    : `<p class="law">Fejl: ${input.audit.errors.map(escapeHtml).join("; ")}</p>`;
  return `<section class="card">
<h2>1. Integritet af bogføringen</h2>
<div class="metric"><span class="k">Audit-kæde verificeret</span><span class="v">${auditPill}</span></div>
<div class="metric"><span class="k">Antal posterede entries</span><span class="v">${input.audit.entryCount}</span></div>
${errors}
<p>Rentemester verificerer hash-kæden over alle posterede bogføringsposter ved at replaye SHA-256 over hver posts kanoniske JSON. Hver post bærer en reference til sin forgænger; en mutation af en gammel post brydes mod den næste hash. Rettelser bogføres som en ny linket modpostering (<code>reversal_of_entry_id</code>) — originalen ændres aldrig.</p>
<p class="law"><strong>Hjemmel:</strong> Bogføringsloven § 13 stk. 1 (sikring mod ødelæggelse, fejl og misbrug) og § 9 stk. 3 (rettelser skal vises tydeligt). Verificeres af <code>rentemester audit verify</code>.</p>
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
      return `<tr><td>${escapeHtml(d.label)}</td><td>${escapeHtml(d.kind)}</td>` +
        `<td>${region}</td><td>${security}</td></tr>`;
    })
    .join("\n");
  const destTable = destinations.length === 0
    ? `<p>Ingen destinationer registreret.</p>`
    : `<table><thead><tr><th>Label</th><th>Type</th><th>EU/EØS</th><th>IT-sikkerhed</th></tr></thead><tbody>${destinations}</tbody></table>`;
  return `<section class="card">
<h2>2. Opbevaring og backup</h2>
<div class="metric"><span class="k">Backup-pligt opfyldt</span><span class="v">${overallPill}</span></div>
<div class="metric"><span class="k">Seneste backup-ID</span><span class="v">${escapeHtml(lastBackup)}</span></div>
<div class="metric"><span class="k">Senest placeret offsite</span><span class="v">${offsitePill}</span></div>
<div class="metric"><span class="k">Antal destinationer</span><span class="v">${b.destinationCount}</span></div>
${destTable}
<p>Backups signeres med HMAC-SHA256 (default) eller Ed25519 (opt-in). Den offentlige nøgle pakkes ind i backup-arkivet så en revisor kan verificere uden adgang til den private nøgle.</p>
<p class="law"><strong>Hjemmel:</strong> Bogføringsloven § 12 stk. 1 (5-årig betryggende opbevaring) og BEK 205 § 4 stk. 1-2 (ugentlig fuld sikkerhedskopi hos ikke-nærtstående tredjepart på EU/EØS-server). Vejledning: <code>rentemester system backup-guide</code>.</p>
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
      `<tr><td>${escapeHtml(labelOf(row.table))}</td><td>${row.total}</td>` +
      `<td>${row.expired}</td><td>${escapeHtml(row.nextExpiry ?? "—")}</td>` +
      `<td>${escapeHtml(row.oldestExpired ?? "—")}</td></tr>`
    )
    .join("\n");
  return `<section class="card">
<h2>3. Opbevaringsfrist (5 år)</h2>
<div class="metric"><span class="k">Status pr. ${escapeHtml(r.asOf)}</span><span class="v">${overallPill}</span></div>
<table>
<thead><tr><th>Materiale</th><th>Total</th><th>Udløbet</th><th>Næste udløb</th><th>Ældste udløbet</th></tr></thead>
<tbody>${rows}</tbody>
</table>
<p>Retain-until beregnes som udgangen af regnskabsåret plus 5 kalenderår. Bogføringsposter, dokumenter og banktransaktioner får hver deres deadline. Materiale inden for fristen kan ikke slettes — heller ikke ved en GDPR-anmodning (§ 13 stk. 1).</p>
<p class="law"><strong>Hjemmel:</strong> Bogføringsloven § 12 stk. 1. Liste: <code>rentemester retention status</code>.</p>
</section>`;
}

function periodSection(input: ComplianceReportInput): string {
  return `<section class="card">
<h2>4. Periode-lukning</h2>
<div class="metric"><span class="k">Lukkede regnskabsperioder</span><span class="v">${input.periods.closedCount}</span></div>
<div class="metric"><span class="k">Senest lukkede periode</span><span class="v">${escapeHtml(input.periods.lastClosedLabel ?? "—")}</span></div>
<p>En lukket periode kan ikke modtage nye eller ændrede posteringer. Genåbning bogføres som en audit-event (immutable) med eksplicit grund — selve lukke-rækken muteres aldrig. Periodens effektive tilstand replayes fra audit-historikken hvert gang den evalueres.</p>
<p class="law"><strong>Hjemmel:</strong> BEK 205 § 3 stk. 3. Implementering: <code>rentemester period close</code> / <code>period reopen</code>.</p>
</section>`;
}

function gdprSection(input: ComplianceReportInput): string {
  return `<section class="card">
<h2>5. GDPR — persondata</h2>
<div class="metric"><span class="k">GDPR-events i audit-log</span><span class="v">${input.gdpr.eventCount}</span></div>
<div class="metric"><span class="k">Audit-fingerprint</span><span class="v">${escapeHtml(input.gdpr.fingerprint)}</span></div>
<p>Indsigt (art. 15) leveres som en signed JSON-pakke via <code>rentemester gdpr export</code>. Sletning (art. 17) gemmes som en append-only tombstone i <code>gdpr_erasures</code>; bogføringspligtige felter afvises indtil retention-fristen er udløbet (GDPR art. 17 stk. 3 lit. b — retlig forpligtelse).</p>
<p class="law"><strong>Hjemmel:</strong> GDPR (EU 2016/679) art. 15 og 17. Audit: <code>rentemester gdpr audit-log</code>.</p>
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
        `<tr><td>${escapeHtml(s.sourceId)}</td>` +
        `<td>${s.inScopeCitedCount}/${s.inScopeOperativeCount}</td></tr>`,
    )
    .join("\n");
  return `<section class="card">
<h2>6. Regulatorisk dækning</h2>
<div class="metric"><span class="k">In-scope dækning</span><span class="v">${c.overall.inScopeCitedCount}/${c.overall.inScopeOperativeCount} (${pct})</span></div>
<div class="metric"><span class="k">Closure / drift / scope errors</span><span class="v">${integrityPill} (${c.closureErrors.length}/${c.driftErrors.length}/${c.scopeErrors.length})</span></div>
<div class="metric"><span class="k">Uncited regler (allowlisted)</span><span class="v">${c.uncitedRules.length}</span></div>
<p>Tallet måler hvor stor en del af de in-scope danske lovbestemmelser der citeres af en eksekverbar regel. Tælleren stiger når en ny paragraf cites; nævneren er den scope-erklærede delmængde af lovkorpusset (se <code>sources/scope.yaml</code>).</p>
<table>
<thead><tr><th>Kilde</th><th>Citeret / In-scope</th></tr></thead>
<tbody>${perSourceRows}</tbody>
</table>
<p>Kommando: <code>rentemester reg coverage</code>. Verbatim citation-review: <code>rentemester reg citations</code>.</p>
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
        return `<tr><td><code>${escapeHtml(rule.ruleId)}</code><br><span class="sub">${escapeHtml(rule.name)}</span></td><td>${cites}</td></tr>`;
      })
      .join("\n");
    blocks.push(
      `<h3>${escapeHtml(category)}</h3>` +
      `<table><thead><tr><th>Regel</th><th>Citation</th></tr></thead><tbody>${ruleRows}</tbody></table>`,
    );
  }
  return `<section class="card">
<h2>7. Regler og deres lovhjemmel</h2>
<p>For hver implementeret bogføringsregel vises ID, beskrivelse og den paragraf den citerer. En revisor kan opklare det forretningsmæssige spørgsmål "hvor i loven står det?" ved at slå reglens ID op her.</p>
${blocks.join("\n")}
</section>`;
}

function authoritySection(): string {
  return `<section class="card">
<h2>8. Myndighedsudlevering og SAF-T</h2>
<p>Rentemester kan på anmodning levere bogføringsmaterialet til en myndighed (Skattestyrelsen, Erhvervsstyrelsen) som en deterministisk eksport-pakke med SHA-256-manifest. Frist: 4 uger fra anmodningstidspunktet, jf. BEK 97 § 11 stk. 1.</p>
<ul>
<li><code>rentemester system export-authority --requested-at &lt;iso&gt; --requester "Skattestyrelsen" --out &lt;dir&gt;</code> — fuld pakke med bogføringsposter, bilag, banktransaktioner, audit-log og manifest.</li>
<li><code>rentemester system export-saft --from YYYY-MM-DD --to YYYY-MM-DD --out &lt;dir&gt;</code> — SAF-T-eksport (Standard Audit File for Tax) for kontoplan, journal, salgs- og indkøbsfakturaer samt master-files.</li>
<li><code>rentemester system export-accountant --out &lt;dir&gt;</code> — håndoff-pakke til revisor eller bogholder.</li>
</ul>
<p class="law"><strong>Hjemmel:</strong> BEK 97 §§ 10-11 (digitale standard bogføringssystemer skal stille materialet til rådighed inden for 4 uger).</p>
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
<html lang="da">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Compliance-rapport — ${escapeHtml(input.companyName)}</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono&family=IBM+Plex+Sans:wght@400;600&family=Source+Serif+4:wght@600&display=swap" rel="stylesheet">
<style>${STYLE}</style>
</head>
<body>
<main>
<h1>Compliance-rapport</h1>
<p class="sub">${escapeHtml(input.companyName)}${cvr} · ${fiscalYear}Genereret ${escapeHtml(formatInstant(input.generatedAt))}</p>

<section class="card">
<h2>Forretningsmæssigt overblik</h2>
<p>Denne rapport dokumenterer hvordan Rentemester driver bogføringen i overensstemmelse med dansk lovgivning. Rapporten er en deterministisk funktion af virksomhedens nuværende ledger-tilstand og det regel-bundle der er installeret — samme input giver byte-for-byte identisk output. Hash i bunden tillader en revisor at konstatere at en udleveret kopi ikke er ændret.</p>
<p>Rapporten dækker:</p>
<ul>
<li>Integritet af bogføringen (hash-kæde + append-only)</li>
<li>Opbevaring og backup (5 års betryggende opbevaring + ugentlig sikkerhedskopi)</li>
<li>Periode-lukning og rettelser via reversal</li>
<li>GDPR-håndtering og retention-grænse for sletning</li>
<li>Regulatorisk dækning (citationer mod lovkorpusset)</li>
<li>Liste af regler og deres paragraf-hjemmel</li>
<li>Myndighedsudlevering, SAF-T og revisor-eksport</li>
</ul>
</section>

${body}

<footer>
<div class="metric"><span class="k">Rule bundle</span><span class="v">${escapeHtml(input.ruleBundleVersion)}</span></div>
<div class="metric"><span class="k">Commit</span><span class="v">${escapeHtml(input.commitSha ?? "—")}</span></div>
<div class="metric"><span class="k">Rapport-fingerprint</span><span class="v">sha256:${fingerprint}</span></div>
<p>Denne rapport er vejledende dokumentation til revisor eller myndighed. Den endelige juridiske vurdering er virksomhedens og dens rådgivers ansvar.</p>
</footer>
</main>
</body>
</html>
`;
}

export function complianceReportFingerprint(html: string): string {
  return createHash("sha256").update(html, "utf8").digest("hex");
}
