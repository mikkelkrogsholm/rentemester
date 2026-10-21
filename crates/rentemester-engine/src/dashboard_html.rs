//! Standalone dashboard presentation. All accounting values and availability
//! explanations are supplied by the admitted read projection; rendering has no IO.
use crate::{Error, calendar::valid_iso_date};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dashboard {
    pub company_name: String,
    pub cvr: Option<String>,
    pub as_of_date: String,
    pub generated_at: String,
    pub build_revision: String,
    pub rule_bundle_version: String,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub basis: String,
    pub facts: Vec<Fact>,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub notes: Vec<String>,
    pub next_action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    pub label: String,
    pub value: String,
}

/// Render the complete supplied snapshot, preserving section order and all notes.
/// No availability or accounting conclusion is inferred from an empty collection.
pub fn render(dashboard: &Dashboard) -> Result<String, Error> {
    if dashboard.company_name.trim().is_empty() || !valid_iso_date(&dashboard.as_of_date) {
        return Err(invalid("Virksomhedsnavn og gyldig opgørelsesdato kræves"));
    }
    for section in &dashboard.sections {
        if section.id.trim().is_empty()
            || section.title.trim().is_empty()
            || section.basis.trim().is_empty()
            || section
                .rows
                .iter()
                .any(|row| section.columns.is_empty() || row.len() != section.columns.len())
        {
            return Err(invalid(
                "Sektion kræver id, titel, grundlag og ens tabelbredde",
            ));
        }
    }
    let mut html = String::from(
        "<!doctype html>\n<html lang=\"da\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>Dashboard · ",
    );
    escape(&mut html, &dashboard.company_name);
    html.push_str("</title><style>");
    html.push_str(STYLE);
    html.push_str("</style></head><body><main><header><p class=\"eyebrow\">Rentemester · Økonomisk overblik</p><h1>");
    escape(&mut html, &dashboard.company_name);
    html.push_str("</h1><p class=\"header-meta\">");
    if let Some(cvr) = &dashboard.cvr {
        html.push_str("CVR ");
        escape(&mut html, cvr);
        html.push_str(" · ");
    }
    html.push_str("Opgørelsesdato: ");
    escape(&mut html, &dashboard.as_of_date);
    html.push_str("</p><p class=\"scope\">Grundlag og tidsafgrænsning fremgår af hver sektion. Aktuelle oplysninger er ikke nødvendigvis et historisk øjebliksbillede.</p></header><nav aria-label=\"Sektioner\"><ul>");
    for (index, section) in dashboard.sections.iter().enumerate() {
        // Anchors are generated only from indices, never from caller-supplied IDs.
        write!(html, "<li><a href=\"#section-{index}\">").unwrap();
        escape(&mut html, &section.title);
        html.push_str("</a></li>");
    }
    html.push_str("</ul></nav><div class=\"sections\">");
    for (index, section) in dashboard.sections.iter().enumerate() {
        write!(html, "<section id=\"section-{index}\" data-section=\"").unwrap();
        escape(&mut html, &section.id);
        write!(
            html,
            "\" aria-labelledby=\"heading-{index}\"><h2 id=\"heading-{index}\">"
        )
        .unwrap();
        escape(&mut html, &section.title);
        html.push_str("</h2><p class=\"basis\"><strong>Grundlag: </strong>");
        escape(&mut html, &section.basis);
        html.push_str("</p>");
        if !section.facts.is_empty() {
            html.push_str("<dl class=\"facts\">");
            for fact in &section.facts {
                html.push_str("<div><dt>");
                escape(&mut html, &fact.label);
                html.push_str("</dt><dd>");
                escape(&mut html, &fact.value);
                html.push_str("</dd></div>");
            }
            html.push_str("</dl>");
        }
        if !section.columns.is_empty() {
            html.push_str("<div class=\"table-scroll\"><table><caption>");
            escape(&mut html, &section.title);
            html.push_str("</caption><thead><tr>");
            for column in &section.columns {
                html.push_str("<th scope=\"col\">");
                escape(&mut html, column);
                html.push_str("</th>");
            }
            html.push_str("</tr></thead><tbody>");
            if section.rows.is_empty() {
                write!(html, "<tr><td colspan=\"{}\" class=\"empty\">Ingen poster i det viste grundlag.</td></tr>", section.columns.len()).unwrap();
            }
            for row in &section.rows {
                html.push_str("<tr>");
                for cell in row {
                    html.push_str("<td>");
                    escape(&mut html, cell);
                    html.push_str("</td>");
                }
                html.push_str("</tr>");
            }
            html.push_str("</tbody></table></div>");
        }
        if !section.notes.is_empty() {
            html.push_str("<ul class=\"notes\">");
            for note in &section.notes {
                html.push_str("<li>");
                escape(&mut html, note);
                html.push_str("</li>");
            }
            html.push_str("</ul>");
        }
        if let Some(action) = &section.next_action {
            html.push_str("<p class=\"next-action\"><strong>Næste handling: </strong>");
            escape(&mut html, action);
            html.push_str("</p>");
        }
        html.push_str("</section>");
    }
    html.push_str("</div><footer><p>Genereret: ");
    escape(&mut html, &dashboard.generated_at);
    html.push_str("</p><dl><div><dt>Buildrevision</dt><dd>");
    escape(&mut html, &dashboard.build_revision);
    html.push_str("</dd></div><div><dt>Regelpakke</dt><dd>");
    escape(&mut html, &dashboard.rule_bundle_version);
    html.push_str("</dd></div></dl></footer></main></body></html>\n");
    Ok(html)
}

fn invalid(message: &str) -> Error {
    Error::new("invalid_dashboard", message)
}

fn escape(output: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(character),
        }
    }
}

const STYLE: &str = r#"
:root{color-scheme:light;font-family:system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:#172a35;background:#f3f6f8;line-height:1.55}
*{box-sizing:border-box}body{margin:0}main{max-width:1140px;margin:auto;padding:40px 24px}header{margin-bottom:24px}h1,h2,p{margin-top:0}h1{font-size:clamp(1.8rem,4vw,2.8rem);line-height:1.15;overflow-wrap:anywhere;margin-bottom:12px}h2{font-size:1.25rem;color:#134d59;margin-bottom:12px}.eyebrow{color:#386775;font-size:.8rem;font-weight:700;letter-spacing:.08em;text-transform:uppercase}.header-meta{font-weight:600}.scope,.basis,footer,.notes{color:#435965}.scope{max-width:80ch}.basis{font-size:.9rem}nav ul{display:flex;flex-wrap:wrap;gap:8px 18px;list-style:none;padding:0;margin:0 0 28px}a{color:#145f70;text-underline-offset:3px}a:focus-visible{outline:3px solid #145f70;outline-offset:4px}.sections{display:grid;gap:20px}section{background:white;border:1px solid #d5e0e5;border-radius:12px;padding:24px;scroll-margin-top:16px;overflow-wrap:anywhere}.facts{display:flex;flex-wrap:wrap;gap:20px 36px;margin:20px 0}.facts div{flex:1 1 160px;min-width:0}dt{font-size:.85rem;color:#435965}dd{margin:4px 0 0}.facts dd{font-size:1.45rem;font-weight:650;font-variant-numeric:tabular-nums}.table-scroll{overflow-x:auto}table{width:100%;border-collapse:collapse;font-size:.92rem;text-align:left}caption{text-align:left;font-weight:600;padding-bottom:8px}th,td{padding:10px 12px;border-bottom:1px solid #dce5e9;vertical-align:top}th{background:#edf4f5;font-weight:650}td{font-variant-numeric:tabular-nums}.empty{color:#435965;font-style:italic}.notes{padding-left:22px}.notes li+li{margin-top:6px}.next-action{background:#edf5f7;border-left:3px solid #317989;padding:12px 14px;margin:18px 0 0}footer{border-top:1px solid #cddae0;margin-top:28px;padding-top:20px;font-size:.8rem;overflow-wrap:anywhere}footer dl{display:flex;flex-wrap:wrap;gap:12px 32px}footer dd{font-family:ui-monospace,monospace}@media(max-width:600px){main{padding:24px 12px}section{padding:18px 14px;border-radius:8px}th,td{padding:8px}.facts{gap:16px}}@media print{:root{background:white;color:black;font-size:10pt}main{max-width:none;padding:0}nav{display:none}section{border:1px solid #bbb;border-radius:0;box-shadow:none;padding:14px;break-inside:avoid}.table-scroll{overflow:visible}table{table-layout:fixed}thead{display:table-header-group}tr{break-inside:avoid}h2{break-after:avoid}a{color:inherit}.next-action,th{background:white}footer{margin-top:18px}}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Dashboard {
        Dashboard {
            company_name: "Eksempel ApS".into(),
            cvr: Some("12345678".into()),
            as_of_date: "2024-02-29".into(),
            generated_at: "2024-03-01T09:00:00Z".into(),
            build_revision: "revision-123".into(),
            rule_bundle_version: "regler-7".into(),
            sections: [
                ("metrics", "Nøgletal"),
                ("exceptions", "Åbne undtagelser"),
                ("vat", "Næste momsfrist"),
                ("invoices", "Åbne fakturaer"),
                ("payables", "Åbne leverandørposter"),
                ("accruals", "Periodiseringer"),
                ("budget", "Budget"),
                ("liquidity", "Likviditet"),
                ("tax", "Skat"),
                ("eu", "EU-salg og OSS"),
                ("activity", "Seneste aktivitet"),
                ("system", "Systemstatus"),
            ]
            .into_iter()
            .map(|(id, title)| Section {
                id: id.into(),
                title: title.into(),
                basis: format!("{id}: aktuelt grundlag med valgt skæringsdato"),
                facts: vec![Fact {
                    label: format!("{id}: antal"),
                    value: "4 · 1.250,00 DKK".into(),
                }],
                columns: vec!["Beskrivelse".into(), "Værdi".into()],
                rows: vec![vec![format!("{id}: række"), "250,00 DKK".into()]],
                notes: vec![format!("{id}: afgrænsning og kilde")],
                next_action: Some(format!("{id}: gennemgå dokumentation")),
            })
            .collect(),
        }
    }

    #[test]
    fn complete_snapshot_preserves_all_sections_and_metadata_deterministically() {
        let dashboard = fixture();
        let html = render(&dashboard).unwrap();
        assert_eq!(html, render(&dashboard).unwrap());
        assert!(html.starts_with("<!doctype html>\n<html lang=\"da\">"));
        assert!(html.contains("<style>"));
        assert!(html.contains("@media print"));
        assert!(html.contains("@media(max-width:600px)"));
        for value in [
            "Eksempel ApS",
            "12345678",
            "2024-02-29",
            "2024-03-01T09:00:00Z",
            "revision-123",
            "regler-7",
        ] {
            assert!(html.contains(value), "missing {value}");
        }
        assert_eq!(html.matches("<section ").count(), 12);
        for (index, section) in dashboard.sections.iter().enumerate() {
            assert!(html.contains(&format!("href=\"#section-{index}\"")));
            assert!(html.contains(&format!("data-section=\"{}\"", section.id)));
            for value in [
                &section.title,
                &section.basis,
                &section.facts[0].label,
                &section.facts[0].value,
                &section.columns[0],
                &section.columns[1],
                &section.rows[0][0],
                &section.rows[0][1],
                &section.notes[0],
                section.next_action.as_ref().unwrap(),
            ] {
                assert!(html.contains(value), "missing {value}");
            }
        }
        for forbidden in [
            "<script", "<link", "<iframe", "https://", "http://", "url(", "@import",
        ] {
            assert!(
                !html.contains(forbidden),
                "external dependency: {forbidden}"
            );
        }
    }

    #[test]
    fn empty_available_table_and_explicit_unavailability_remain_distinct() {
        let mut dashboard = fixture();
        dashboard.sections.truncate(2);
        dashboard.sections[0].rows.clear();
        let unavailable = &mut dashboard.sections[1];
        unavailable.title = "Skat".into();
        unavailable.basis = "Ikke tilgængelig: skatteberegning er ikke implementeret".into();
        unavailable.facts.clear();
        unavailable.rows.clear();
        unavailable.columns.clear();
        unavailable.notes = vec!["Ingen skatteopgørelse er beregnet eller indsendt".into()];
        unavailable.next_action = Some("Indhent en skatteopgørelse hos revisor".into());
        let html = render(&dashboard).unwrap();
        assert_eq!(
            html.matches("Ingen poster i det viste grundlag.").count(),
            1
        );
        assert!(html.contains("Ikke tilgængelig: skatteberegning er ikke implementeret"));
        assert!(html.contains("Ingen skatteopgørelse er beregnet eller indsendt"));
        assert!(html.contains("Indhent en skatteopgørelse hos revisor"));
        assert_eq!(html.matches("<table>").count(), 1);
        let unavailable_html = html.split("id=\"section-1\"").nth(1).unwrap();
        assert!(!unavailable_html.contains("0,00"));
        dashboard.cvr = None;
        assert!(!render(&dashboard).unwrap().contains("CVR "));
    }

    #[test]
    fn escapes_every_supplied_text_and_attribute_location() {
        let mut dashboard = fixture();
        dashboard.sections.truncate(1);
        let payload = "<script src=\"x\">&'\"</script>";
        let escaped = "&lt;script src=&quot;x&quot;&gt;&amp;&#39;&quot;&lt;/script&gt;";
        dashboard.company_name = payload.into();
        dashboard.cvr = Some(payload.into());
        dashboard.generated_at = payload.into();
        dashboard.build_revision = payload.into();
        dashboard.rule_bundle_version = payload.into();
        let section = &mut dashboard.sections[0];
        section.id = payload.into();
        section.title = payload.into();
        section.basis = payload.into();
        section.facts = vec![Fact {
            label: payload.into(),
            value: payload.into(),
        }];
        section.columns = vec![payload.into()];
        section.rows = vec![vec![payload.into()]];
        section.notes = vec![payload.into()];
        section.next_action = Some(payload.into());
        let html = render(&dashboard).unwrap();
        assert!(!html.contains(payload));
        assert!(!html.contains("<script"));
        // Company appears twice; title appears in nav, heading, and caption.
        assert_eq!(html.matches(escaped).count(), 17);
        assert!(html.contains(&format!("data-section=\"{escaped}\"")));
        assert!(html.contains("id=\"section-0\""));
        assert!(html.contains("href=\"#section-0\""));
        dashboard.as_of_date = payload.into();
        assert_eq!(render(&dashboard).unwrap_err().code, "invalid_dashboard");
    }

    #[test]
    fn rejects_invalid_dates_headers_and_table_dimensions() {
        for date in ["2023-02-29", "2024-13-01", "2024-04-31", "2024-2-29"] {
            let mut dashboard = fixture();
            dashboard.as_of_date = date.into();
            assert!(render(&dashboard).is_err(), "accepted {date}");
        }
        let mut dashboard = fixture();
        dashboard.company_name = "  ".into();
        assert!(render(&dashboard).is_err());
        let mut dashboard = fixture();
        dashboard.sections[0].rows[0].pop();
        assert!(render(&dashboard).is_err());
        dashboard.sections[0].columns.clear();
        assert!(render(&dashboard).is_err());
        for field in ["id", "title", "basis"] {
            let mut dashboard = fixture();
            let section = &mut dashboard.sections[0];
            match field {
                "id" => section.id.clear(),
                "title" => section.title.clear(),
                _ => section.basis.clear(),
            }
            assert!(render(&dashboard).is_err(), "accepted empty {field}");
        }
    }
}
