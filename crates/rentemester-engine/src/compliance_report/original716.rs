//! Pure source HTML renderer. No clock, filesystem, authority or backup invention.
use super::{escape, fingerprint};
use crate::Error;
use serde_json::Value;
fn text(v: &Value) -> String {
    if v.is_null() {
        String::new()
    } else if let Some(s) = v.as_str() {
        s.into()
    } else {
        v.to_string()
    }
}
fn e(v: &Value) -> String {
    escape(&text(v))
}
fn fallback(v: &Value) -> String {
    if v.is_null() { "—".into() } else { e(v) }
}
fn pill(kind: &str, text: &str) -> String {
    format!("<span class=\"pill {}\">{}</span>", kind, escape(text))
}
fn template(s: &str, pairs: &[(&str, String)]) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        let Some(end) = tail.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let key = &tail[..end];
        if let Some((_, value)) = pairs.iter().find(|(k, _)| *k == key) {
            out.push_str(value);
        } else {
            out.push_str(&rest[start..start + end + 3]);
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}
pub fn render(input: &Value) -> Result<String, Error> {
    if input["format"] != "rentemester-company-compliance/1" {
        return Err(Error::new("report_invalid", "Unsupported report evidence"));
    }
    let audit = &input["audit"];
    let b = &input["backup"];
    let r = &input["retention"];
    let c = &input["coverage"];
    let audit_pill = if audit["ok"] == true {
        pill("ok", "Verificeret")
    } else {
        pill("bad", "Brudt kæde")
    };
    let errors = if audit["errors"].as_array().is_none_or(Vec::is_empty) {
        String::new()
    } else {
        format!(
            "<p class=\"law\">Fejl: {}</p>",
            audit["errors"]
                .as_array()
                .unwrap()
                .iter()
                .map(e)
                .collect::<Vec<_>>()
                .join("; ")
        )
    };
    let audit_html = template(
        r###"<section class="card">
<h2>1. Integritet af bogføringen</h2>
<div class="metric"><span class="k">Audit-kæde verificeret</span><span class="v">${auditPill}</span></div>
<div class="metric"><span class="k">Antal posterede entries</span><span class="v">${input.audit.entryCount}</span></div>
${errors}
<p>Rentemester verificerer hash-kæden over alle posterede bogføringsposter ved at replaye SHA-256 over hver posts kanoniske JSON. Hver post bærer en reference til sin forgænger; en mutation af en gammel post brydes mod den næste hash. Rettelser bogføres som en ny linket modpostering (<code>reversal_of_entry_id</code>) — originalen ændres aldrig.</p>
<p class="law"><strong>Hjemmel:</strong> Bogføringsloven § 13 stk. 1 (sikring mod ødelæggelse, fejl og misbrug) og § 9 stk. 3 (rettelser skal vises tydeligt). Verificeres af <code>rentemester audit verify</code>.</p>
</section>"###,
        &[
            (r###"auditPill"###, audit_pill),
            (r###"input.audit.entryCount"###, text(&audit["entries"])),
            (r###"errors"###, errors),
        ],
    );
    let destinations = b["destinations"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                e(&d["label"]),
                e(&d["kind"]),
                if d["regionAttestation"]["inEeaOrEu"] == true {
                    format!("✔ {}", e(&d["regionAttestation"]["country"]))
                } else {
                    "—".into()
                },
                if d["itSecurityAttestation"]["meetsRecognisedStandards"] == true {
                    "✔"
                } else {
                    "—"
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let dest_table = if destinations.is_empty() {
        "<p>Ingen destinationer registreret.</p>".into()
    } else {
        format!(
            "<table><thead><tr><th>Label</th><th>Type</th><th>EU/EØS</th><th>IT-sikkerhed</th></tr></thead><tbody>{destinations}</tbody></table>"
        )
    };
    let backup_html = if b["detailPermissionGranted"] == false {
        format!(
            "<section class=\"card\"><h2>2. Opbevaring og backup</h2><p>{}</p></section>",
            e(&b["reason"])
        )
    } else {
        template(
            r###"<section class="card">
<h2>2. Opbevaring og backup</h2>
<div class="metric"><span class="k">Backup-pligt opfyldt</span><span class="v">${overallPill}</span></div>
<div class="metric"><span class="k">Seneste backup-ID</span><span class="v">${escapeHtml(lastBackup)}</span></div>
<div class="metric"><span class="k">Senest placeret offsite</span><span class="v">${offsitePill}</span></div>
<div class="metric"><span class="k">Antal destinationer</span><span class="v">${b.destinationCount}</span></div>
${destTable}
<p>Backups signeres med HMAC-SHA256 (default) eller Ed25519 (opt-in). Den offentlige nøgle pakkes ind i backup-arkivet så en revisor kan verificere uden adgang til den private nøgle.</p>
<p class="law"><strong>Hjemmel:</strong> Bogføringsloven § 12 stk. 1 (5-årig betryggende opbevaring) og BEK 205 § 4 stk. 1-2 (ugentlig fuld sikkerhedskopi hos ikke-nærtstående tredjepart på EU/EØS-server). Vejledning: <code>rentemester system backup-guide</code>.</p>
</section>"###,
            &[
                (
                    r###"overallPill"###,
                    if b["hasCompliantDestination"] == true {
                        pill("ok", "Opfyldt")
                    } else {
                        pill("bad", "Ikke opfyldt")
                    },
                ),
                (
                    r###"escapeHtml(lastBackup)"###,
                    fallback(&b["compliance"]["latestBackupId"]),
                ),
                (
                    r###"offsitePill"###,
                    if b["latestBackupPlacedOffsite"] == true {
                        pill("ok", "Ja")
                    } else {
                        pill("warn", "Nej")
                    },
                ),
                (r###"b.destinationCount"###, text(&b["destinationCount"])),
                (r###"destTable"###, dest_table),
            ],
        )
    };
    let expired: u64 = r["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["expired"].as_u64())
        .sum();
    let rows = r["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                match row["table"].as_str() {
                    Some("documents") => "Bilag",
                    Some("journal_entries") => "Bogføringsposter",
                    _ => "Banktransaktioner",
                },
                text(&row["total"]),
                text(&row["expired"]),
                fallback(&row["nextExpiry"]),
                fallback(&row["oldestExpired"])
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let retention_html = template(
        r###"<section class="card">
<h2>3. Opbevaringsfrist (5 år)</h2>
<div class="metric"><span class="k">Status pr. ${escapeHtml(r.asOf)}</span><span class="v">${overallPill}</span></div>
<table>
<thead><tr><th>Materiale</th><th>Total</th><th>Udløbet</th><th>Næste udløb</th><th>Ældste udløbet</th></tr></thead>
<tbody>${rows}</tbody>
</table>
<p>Retain-until beregnes som udgangen af regnskabsåret plus 5 kalenderår. Bogføringsposter, dokumenter og banktransaktioner får hver deres deadline. Materiale inden for fristen kan ikke slettes — heller ikke ved en GDPR-anmodning (§ 13 stk. 1).</p>
<p class="law"><strong>Hjemmel:</strong> Bogføringsloven § 12 stk. 1. Liste: <code>rentemester retention status</code>.</p>
</section>"###,
        &[
            (r###"escapeHtml(r.asOf)"###, e(&r["asOf"])),
            (
                r###"overallPill"###,
                if expired == 0 {
                    pill("ok", "Inden for fristen")
                } else {
                    pill("warn", &format!("{expired} udløbet"))
                },
            ),
            (r###"rows"###, rows),
        ],
    );
    let period_html = template(
        r###"<section class="card">
<h2>4. Periode-lukning</h2>
<div class="metric"><span class="k">Lukkede regnskabsperioder</span><span class="v">${input.periods.closedCount}</span></div>
<div class="metric"><span class="k">Senest lukkede periode</span><span class="v">${escapeHtml(input.periods.lastClosedLabel ?? "—")}</span></div>
<p>En lukket periode kan ikke modtage nye eller ændrede posteringer. Genåbning bogføres som en audit-event (immutable) med eksplicit grund — selve lukke-rækken muteres aldrig. Periodens effektive tilstand replayes fra audit-historikken hvert gang den evalueres.</p>
<p class="law"><strong>Hjemmel:</strong> BEK 205 § 3 stk. 3. Implementering: <code>rentemester period close</code> / <code>period reopen</code>.</p>
</section>"###,
        &[
            (
                r###"input.periods.closedCount"###,
                text(&input["sourcePeriods"]["closedCount"]),
            ),
            (
                r###"escapeHtml(input.periods.lastClosedLabel ?? "—")"###,
                fallback(&input["sourcePeriods"]["lastClosedLabel"]),
            ),
        ],
    );
    let gdpr_html = template(
        r###"<section class="card">
<h2>5. GDPR — persondata</h2>
<div class="metric"><span class="k">GDPR-events i audit-log</span><span class="v">${input.gdpr.eventCount}</span></div>
<div class="metric"><span class="k">Audit-fingerprint</span><span class="v">${escapeHtml(input.gdpr.fingerprint)}</span></div>
<p>Indsigt (art. 15) leveres som en signed JSON-pakke via <code>rentemester gdpr export</code>. Sletning (art. 17) gemmes som en append-only tombstone i <code>gdpr_erasures</code>; bogføringspligtige felter afvises indtil retention-fristen er udløbet (GDPR art. 17 stk. 3 lit. b — retlig forpligtelse).</p>
<p class="law"><strong>Hjemmel:</strong> GDPR (EU 2016/679) art. 15 og 17. Audit: <code>rentemester gdpr audit-log</code>.</p>
</section>"###,
        &[
            (
                r###"input.gdpr.eventCount"###,
                text(&input["gdpr"]["eventCount"]),
            ),
            (
                r###"escapeHtml(input.gdpr.fingerprint)"###,
                e(&input["gdpr"]["fingerprint"]),
            ),
        ],
    );
    let denominator = c["overall"]["inScopeOperativeCount"].as_u64().unwrap_or(0);
    let numerator = c["overall"]["inScopeCitedCount"].as_u64().unwrap_or(0);
    let pct = if denominator == 0 {
        "—".into()
    } else {
        format!("{}%", (numerator * 100 + denominator / 2) / denominator)
    };
    let count = |k: &str| c[k].as_array().map_or(0, Vec::len);
    let per_source = c["perSource"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["inScopeOperativeCount"].as_u64().unwrap_or(0) > 0)
        .map(|s| {
            format!(
                "<tr><td>{}</td><td>{}/{}</td></tr>",
                e(&s["sourceId"]),
                text(&s["inScopeCitedCount"]),
                text(&s["inScopeOperativeCount"])
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let coverage_html = template(
        r###"<section class="card">
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
</section>"###,
        &[
            (r###"c.overall.inScopeCitedCount"###, numerator.to_string()),
            (
                r###"c.overall.inScopeOperativeCount"###,
                denominator.to_string(),
            ),
            (r###"pct"###, pct),
            (
                r###"integrityPill"###,
                if count("closureErrors") + count("driftErrors") + count("scopeErrors") == 0 {
                    pill("ok", "Ingen fejl")
                } else {
                    pill("bad", "Fejl")
                },
            ),
            (
                r###"c.closureErrors.length"###,
                count("closureErrors").to_string(),
            ),
            (
                r###"c.driftErrors.length"###,
                count("driftErrors").to_string(),
            ),
            (
                r###"c.scopeErrors.length"###,
                count("scopeErrors").to_string(),
            ),
            (
                r###"c.uncitedRules.length"###,
                count("uncitedRules").to_string(),
            ),
            (r###"perSourceRows"###, per_source),
        ],
    );
    let mut groups = std::collections::BTreeMap::<String, Vec<&Value>>::new();
    for rule in c["rules"].as_array().into_iter().flatten() {
        groups
            .entry(text(&rule["category"]))
            .or_default()
            .push(rule);
    }
    let mut blocks = vec![];
    for (category, mut rules) in groups {
        rules.sort_by_key(|r| text(&r["ruleId"]));
        let rows = rules
            .iter()
            .map(|rule| {
                let ps = rule["provisions"].as_array();
                let cites = if ps.is_none_or(Vec::is_empty) {
                    "<em>ingen citation — se allowlist</em>".into()
                } else {
                    ps.unwrap()
                        .iter()
                        .map(|p| format!("{} {}", e(&rule["sourceId"]), e(&p["ref"])))
                        .collect::<Vec<_>>()
                        .join("<br>")
                };
                format!(
                    "<tr><td><code>{}</code><br><span class=\"sub\">{}</span></td><td>{}</td></tr>",
                    e(&rule["ruleId"]),
                    e(&rule["name"]),
                    cites
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(format!("<h3>{}</h3><table><thead><tr><th>Regel</th><th>Citation</th></tr></thead><tbody>{rows}</tbody></table>",escape(&category)));
    }
    let citations_html = template(
        r###"<section class="card">
<h2>7. Regler og deres lovhjemmel</h2>
<p>For hver implementeret bogføringsregel vises ID, beskrivelse og den paragraf den citerer. En revisor kan opklare det forretningsmæssige spørgsmål "hvor i loven står det?" ved at slå reglens ID op her.</p>
${blocks.join("\n")}
</section>"###,
        &[(r###"blocks.join("\n")"###, blocks.join("\n"))],
    );
    let authority_html = r###"<section class="card">
<h2>8. Myndighedsudlevering og SAF-T</h2>
<p>Rentemester kan på anmodning levere bogføringsmaterialet til en myndighed (Skattestyrelsen, Erhvervsstyrelsen) som en deterministisk eksport-pakke med SHA-256-manifest. Frist: 4 uger fra anmodningstidspunktet, jf. BEK 97 § 11 stk. 1.</p>
<ul>
<li><code>rentemester system export-authority --requested-at &lt;iso&gt; --requester "Skattestyrelsen" --out &lt;dir&gt;</code> — fuld pakke med bogføringsposter, bilag, banktransaktioner, audit-log og manifest.</li>
<li><code>rentemester system export-saft --from YYYY-MM-DD --to YYYY-MM-DD --out &lt;dir&gt;</code> — SAF-T-eksport (Standard Audit File for Tax) for kontoplan, journal, salgs- og indkøbsfakturaer samt master-files.</li>
<li><code>rentemester system export-accountant --out &lt;dir&gt;</code> — håndoff-pakke til revisor eller bogholder.</li>
</ul>
<p class="law"><strong>Hjemmel:</strong> BEK 97 §§ 10-11 (digitale standard bogføringssystemer skal stille materialet til rådighed inden for 4 uger).</p>
</section>"###;
    let body = audit_html
        + &backup_html
        + &retention_html
        + &period_html
        + &gdpr_html
        + &coverage_html
        + &citations_html
        + authority_html;
    let hash = fingerprint(&body);
    let instant = text(&input["generatedAt"]);
    let formatted = if let Some(display) = input["generatedAtDisplay"].as_str() {
        display.to_string()
    } else if instant.len() >= 16 && instant.is_ascii() && instant.as_bytes().get(10) == Some(&b'T')
    {
        format!("{} {} UTC", &instant[..10], &instant[11..16])
    } else {
        instant
    };
    Ok(template(
        r###"<!doctype html>
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
"###,
        &[
            (
                r###"escapeHtml(input.companyName)"###,
                e(&input["company"]["name"]),
            ),
            (
                r###"STYLE"###,
                r###"
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
"###
                .into(),
            ),
            (
                r###"cvr"###,
                if input["company"]["cvr"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())
                {
                    format!(" (CVR {})", e(&input["company"]["cvr"]))
                } else {
                    String::new()
                },
            ),
            (
                r###"fiscalYear"###,
                input["fiscalYear"]["displayLabel"]
                    .as_str()
                    .map(|s| format!("Regnskabsår: {} · ", escape(s)))
                    .unwrap_or_default(),
            ),
            (
                r###"escapeHtml(formatInstant(input.generatedAt))"###,
                escape(&escape(&formatted)),
            ),
            (r###"body"###, body),
            (
                r###"escapeHtml(input.ruleBundleVersion)"###,
                e(&input["ruleBundleVersion"]),
            ),
            (
                r###"escapeHtml(input.commitSha ?? "—")"###,
                fallback(&input["commitSha"]),
            ),
            (r###"fingerprint"###, hash),
        ],
    ))
}
