import { documentAttr, documentCss, documentFontScript } from "../design/document-html";
// Backup rules page: a deterministic, self-contained HTML page that explains
// — in plain Danish — what the law requires of a backup and where it may be
// stored, then shows the company's live backup/placement status against
// those rules.
//
// Render contract (same as dashboard.ts): given the same input the output is
// byte-for-byte identical. No Date.now(), Math.random(), filesystem or env —
// the caller (CLI/MCP) collects everything and passes it in.

import type { BackupGovernanceStatus } from "./backup-governance";

export type BackupGuideInput = {
  generatedAt: string; // ISO 8601 UTC, supplied by the caller
  companyName: string;
  governance: BackupGovernanceStatus;
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

function pill(kind: "ok" | "warn" | "bad", label: string): string {
  return `<span ${documentAttr(kind === "ok" ? "pillOk" : kind === "warn" ? "pillWarn" : "pillBad")}>${escapeHtml(label)}</span>`;
}

function yesNo(value: boolean): string {
  return value ? "ja" : "nej";
}

function statusSection(governance: BackupGovernanceStatus): string {
  const c = governance.compliance;
  const lock = governance.lock;

  const overall = governance.ok
    ? pill("ok", "Backup-pligten er opfyldt")
    : c.backupDue
      ? pill("bad", "Backup mangler nu")
      : pill("warn", "Backup ikke placeret på attesteret EU/EØS-destination");

  const lockPill = !lock.enforced
    ? pill("warn", "Lås: slået fra (opt-in)")
    : lock.locked
      ? pill("bad", "Lås: bogføring er låst")
      : pill("ok", "Lås: aktiv, ikke udløst");

  const days =
    c.daysSinceLatestBackup === null
      ? "ingen backup endnu"
      : `${c.daysSinceLatestBackup} dage siden seneste backup`;

  const rows = [
    ["Status", overall],
    ["Seneste backup", escapeHtml(formatInstant(c.latestBackupAt))],
    ["Dage siden backup", escapeHtml(days)],
    ["Backup forfalden", c.backupDue ? pill("bad", "ja") : pill("ok", "nej")],
    [
      "Seneste backup på attesteret EU/EØS-destination",
      governance.latestBackupPlacedOffsite ? pill("ok", "ja, verificeret") : pill("bad", "nej"),
    ],
    ["Bogførings-lås", lockPill],
  ];

  const metrics = rows
    .map((r) => `<div ${documentAttr("metric")}><span ${documentAttr("muted")}>${escapeHtml(r[0]!)}</span><span ${documentAttr("metricValue")}>${r[1]}</span></div>`)
    .join("");

  let destinationTable: string;
  if (governance.destinations.length === 0) {
    destinationTable =
      `<p ${documentAttr("muted")}>Ingen backup-destinationer er konfigureret endnu. ` +
      `Tilføj én med <code ${documentAttr("code")}>rentemester system backup-add-destination</code> — ` +
      `og husk EU/EØS-kravet nedenfor.</p>`;
  } else {
    const body = governance.destinations
      .map((d) => {
        const compliant =
          d.regionAttestation.inEeaOrEu && d.nonRelatedParty &&
          d.itSecurityAttestation?.meetsRecognisedStandards === true;
        return (
          `<tr><td ${documentAttr("guideCell")}>${escapeHtml(d.label)}</td><td ${documentAttr("guideCell")}>${escapeHtml(d.kind)}</td>` +
          `<td ${documentAttr("guideCell")}>${escapeHtml(yesNo(d.regionAttestation.inEeaOrEu))}` +
          `${d.regionAttestation.country ? ` (${escapeHtml(d.regionAttestation.country)})` : ""}</td>` +
          `<td ${documentAttr("guideCell")}>${escapeHtml(yesNo(d.nonRelatedParty))}</td>` +
          `<td ${documentAttr("guideCell")}>${compliant ? pill("ok", "§4 opfyldt") : pill("bad", "ikke §4")}</td></tr>`
        );
      })
      .join("");
    destinationTable =
      `<div ${documentAttr("tableScroll")} tabindex="0" role="region" aria-label="Regnskabstabel"><table ${documentAttr("guideTable")}><thead><tr><th ${documentAttr("guideTh")}>Destination</th><th ${documentAttr("guideTh")}>Type</th><th ${documentAttr("guideTh")}>EU/EØS</th>` +
      `<th ${documentAttr("guideTh")}>Ikke-nærtstående</th><th ${documentAttr("guideTh")}>§4</th></tr></thead><tbody>${body}</tbody></table></div>`;
  }

  return (
    `<section ${documentAttr("card")}><h2 ${documentAttr("guideH2")}>Din status nu</h2>` +
    `<p ${documentAttr("sub")}>Tjekket pr. ${escapeHtml(formatInstant(governance.checkedAt))}.</p>` +
    metrics +
    `<h3 ${documentAttr("guideH3")}>Konfigurerede destinationer</h3>${destinationTable}` +
    (lock.enforced && lock.locked
      ? `<p ${documentAttr("lockReason")}>${escapeHtml(lock.reason)} ` +
        `Kør <code ${documentAttr("code")}>rentemester system backup</code> for at låse op; placér derefter arkivet ` +
        `på en EU/EØS-destination for at opfylde § 4.</p>`
      : "") +
    `</section>`
  );
}

export function renderBackupGuide(input: BackupGuideInput): string {
  const { governance } = input;
  return `<!doctype html>
<html lang="da" ${documentAttr("root")}>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Backup og dataopbevaring — ${escapeHtml(input.companyName)}</title>
<style>${documentCss}</style>
${documentFontScript()}
</head>
<body ${documentAttr("guideBody")}>
<main ${documentAttr("backupMain")}>
<h1 ${documentAttr("guideH1")}>Backup og dataopbevaring</h1>
<p ${documentAttr("sub")}>${escapeHtml(input.companyName)} — regler og status</p>

<section ${documentAttr("card")}>
<h2 ${documentAttr("guideH2")}>Hvad kræver loven?</h2>
<p ${documentAttr("muted")}>Det er ikke valgfrit at tage backup. Tre regelsæt gælder:</p>
<h3 ${documentAttr("guideH3")}>Bogføringsloven (LOV 700/2022)</h3>
<ul ${documentAttr("ul")}>
<li ${documentAttr("li")}><strong>§ 12, stk. 1</strong> — regnskabsmateriale skal opbevares <em>på betryggende vis i 5 år</em> fra udgangen af det regnskabsår, det vedrører.</li>
<li ${documentAttr("li")}><strong>§ 15, stk. 1, nr. 2</strong> — et digitalt bogføringssystem skal opfylde <em>anerkendte standarder for it-sikkerhed</em> (bl.a. bruger- og adgangsstyring) og sikre <em>automatisk sikkerhedskopiering</em>.</li>
<li ${documentAttr("li")}><strong>§ 6, stk. 1, nr. 2</strong> — der skal foreligge en skriftlig <em>beskrivelse af proceduren</em> for betryggende opbevaring.</li>
</ul>
<h3 ${documentAttr("guideH3")}>Bekendtgørelse om ikke-registrerede digitale bogføringssystemer (BEK 205/2024)</h3>
<ul ${documentAttr("ul")}>
<li ${documentAttr("li")}><strong>§ 4, stk. 1</strong> — virksomheden skal <em>mindst ugentligt</em> tage en fuld sikkerhedskopi af alle bogførte transaktioner og bilag — medmindre der ikke er bogført noget siden seneste kopi.</li>
<li ${documentAttr("li")}><strong>§ 4, stk. 2</strong> — kopien skal opbevares hos en <em>ikke-nærtstående part</em>, der formodes at opfylde anerkendte it-sikkerhedsstandarder, <em>på en server i et EU- eller EØS-land</em>.</li>
<li ${documentAttr("li")}><strong>§ 4, stk. 3</strong> — stk. 2 gælder ikke, hvis virksomheden allerede er underlagt backup-krav i anden lovgivning.</li>
</ul>
</section>

<section ${documentAttr("card")}>
<h2 ${documentAttr("guideH2")}>Hvor må du gemme din backup?</h2>
<p ${documentAttr("muted")}>En backup-fil, du blot lægger et tilfældigt sted, opfylder ikke nødvendigvis loven. Destinationen skal leve op til <strong>alle tre</strong> krav i BEK 205/2024 § 4, stk. 2:</p>
<ul ${documentAttr("ul")}>
<li ${documentAttr("li")}><strong>EU/EØS-server.</strong> Data skal ligge på en server i et EU- eller EØS-land. Almindelig (consumer) Dropbox og Google Drive giver <em>ikke</em> garanteret dataresidens i EU/EØS — vælg en udbyder eller region, hvor det er bekræftet.</li>
<li ${documentAttr("li")}><strong>Ikke-nærtstående part.</strong> Backuppen må ikke ligge hos dig selv eller en nærtstående — den skal ligge hos en uafhængig tredjepart.</li>
<li ${documentAttr("li")}><strong>Anerkendte it-sikkerhedsstandarder.</strong> Tredjeparten skal kunne formodes at opfylde anerkendte it-sikkerhedsstandarder (fx ISO 27001).</li>
</ul>
<p ${documentAttr("muted")}>Når du tilføjer en destination i Rentemester, bekræfter du som menneske disse forhold — Rentemester kan ikke selv vide, i hvilket land en cloud-mappe ligger. Agenten flytter filerne; du attesterer rammen.</p>
<p ${documentAttr("sub")}>Tip: en Dropbox- eller Google Drive-<em>desktop-mappe</em> er bare en lokal mappe — agenten eller du kan lægge backup-arkivet dér, og synkroniseringen klarer resten.</p>
</section>

${statusSection(governance)}

<footer ${documentAttr("guideFooter")}>
Genereret ${escapeHtml(formatInstant(input.generatedAt))}.
Kilder: bogføringsloven (LOV nr. 700 af 24/05/2022); bekendtgørelse nr. 205 af 04/03/2024.
Denne side er vejledende — den endelige vurdering er virksomhedens og dens rådgivers ansvar.
</footer>
</main>
</body>
</html>
`;
}
