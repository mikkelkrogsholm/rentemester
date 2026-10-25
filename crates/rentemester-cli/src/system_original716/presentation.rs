use super::*;
fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "—".into(),
        _ => v.to_string(),
    }
}
fn heading(label: &str) -> String {
    let first = label
        .split(" — ")
        .next()
        .unwrap_or(label)
        .split(". ")
        .next()
        .unwrap_or(label);
    if first.chars().count() > 60 {
        format!("{}…", first.chars().take(57).collect::<String>())
    } else {
        first.into()
    }
}
pub(super) fn output(command: &str, v: Value, f: &Flags<'_>) -> Output {
    let json = f.contains_key("--json")
        || f.get("--format") == Some(&"json")
        || (f.get("--format") != Some(&"human") && !std::io::stdout().is_terminal());
    let ok = v["ok"] != false;
    if json {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
            stderr: String::new(),
            exit: if ok { 0 } else { 1 },
        };
    }
    if command == "backup-status" {
        return Output {
            stdout: format!("{}\n", status(&v)),
            stderr: String::new(),
            exit: if ok { 0 } else { 1 },
        };
    }
    let s = spec(command).unwrap();
    let label = s["description"].as_str().unwrap();
    if !ok {
        let mut lines = vec![format!("✘ {label} failed")];
        let errors = v["errors"].as_array();
        if errors.is_none_or(|a| a.is_empty()) {
            lines.push("  → Kommandoen fejlede uden en specifik fejlbesked.".into());
        } else {
            for e in errors.unwrap() {
                lines.push(format!("  → {}", text(e)));
            }
        }
        warnings(&mut lines, &v, true);
        return Output {
            stdout: String::new(),
            stderr: format!("{}\n", lines.join("\n")),
            exit: 1,
        };
    }
    let mut lines = vec![format!("✔ {}", heading(label))];
    for (k, label) in [
        ("message", "Besked"),
        ("backupId", "Backup-id"),
        ("exportDir", "Eksportmappe"),
        ("latestBackupId", "Seneste backup-id"),
        ("count", "Antal"),
        ("total", "I alt"),
        ("name", "Navn"),
    ] {
        if v[k].is_string() || v[k].is_number() || v[k].is_boolean() {
            let t = text(&v[k]);
            if !t.is_empty() {
                lines.push(format!("  {label}: {t}"));
            }
        }
    }
    warnings(&mut lines, &v, true);
    Output {
        stdout: format!("{}\n", lines.join("\n")),
        stderr: String::new(),
        exit: 0,
    }
}
fn warnings(lines: &mut Vec<String>, v: &Value, indent: bool) {
    if let Some(a) = v["warnings"].as_array().filter(|a| !a.is_empty()) {
        lines.push(if indent { "  Advarsler:" } else { "Advarsler:" }.into());
        for w in a {
            lines.push(format!(
                "{}- {}",
                if indent { "    " } else { "  " },
                text(w)
            ));
        }
    }
}
fn status(v: &Value) -> String {
    let found = v["backupsFound"].as_u64().unwrap_or(0);
    let activity = v["hasActivitySinceBackup"] == true;
    let due = v["backupDue"] == true;
    let mut l=vec![format!("Backup-status pr. {}",text(&v["checkedAt"])),String::new(),if found==0{if activity{"Backup-pligten er IKKE opfyldt: der er bogført data, men ingen backup er taget endnu."}else{"Backup-pligten er opfyldt: der er endnu ingen bogført data, der kræver backup."}}else if due{"Backup-pligten er IKKE opfyldt: en ny backup er forfalden."}else{"Backup-pligten er opfyldt: seneste backup dækker den nuværende bogføring."}.into(),String::new(),format!("  Seneste backup:                {}",v["latestBackupId"].as_str().filter(|s|!s.is_empty()).unwrap_or("ingen registreret"))];
    if v["latestBackupAt"].is_string() {
        l.push(format!(
            "  Taget:                         {}",
            text(&v["latestBackupAt"])
        ));
    }
    if v["daysSinceLatestBackup"].is_number() {
        l.push(format!(
            "  Dage siden seneste backup:     {}",
            text(&v["daysSinceLatestBackup"])
        ));
    }
    l.push(format!(
        "  Ændringer siden seneste backup: {}",
        if activity { "ja" } else { "nej" }
    ));
    l.push(String::new());
    if found == 0 && activity {
        l.push(
            "Tag en backup nu: der er bogført data, som endnu ikke er sikkerhedskopieret.".into(),
        );
    } else if v["requiredBy"].is_string() {
        l.push(format!(
            "{} {}.",
            if due {
                "Næste backup skulle have været taget senest"
            } else {
                "Næste backup skal tages senest"
            },
            text(&v["requiredBy"])
        ));
    } else if !activity {
        l.push("Ingen frist: backup kræves først, når der bogføres nye data.".into());
    }
    l.push(
        "Backup-pligten er ugentlig — tag en backup mindst hver 7. dag, når der bogføres.".into(),
    );
    warnings(&mut l, v, false);
    l.join("\n")
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn instant(v: &Value) -> String {
    let Some(s) = v.as_str() else {
        return "—".into();
    };
    if s.len() >= 19 && s.is_ascii() && s.as_bytes()[10] == b'T' {
        format!("{} {} UTC", &s[..10], &s[11..19])
    } else {
        s.into()
    }
}
fn pill(kind: &str, label: &str) -> String {
    format!("<span class=\"pill {kind}\">{}</span>", escape(label))
}
fn yes(v: bool) -> &'static str {
    if v { "ja" } else { "nej" }
}
pub(super) fn guide(name: &str, g: &Value) -> String {
    let c = &g["compliance"];
    let lock = &g["lock"];
    let overall = if g["ok"] == true {
        pill("ok", "Backup-pligten er opfyldt")
    } else if c["backupDue"] == true {
        pill("bad", "Backup mangler nu")
    } else {
        pill(
            "warn",
            "Backup ikke placeret på attesteret EU/EØS-destination",
        )
    };
    let lock_pill = if lock["enforced"] != true {
        pill("warn", "Lås: slået fra (opt-in)")
    } else if lock["locked"] == true {
        pill("bad", "Lås: bogføring er låst")
    } else {
        pill("ok", "Lås: aktiv, ikke udløst")
    };
    let days = if c["daysSinceLatestBackup"].is_null() {
        "ingen backup endnu".into()
    } else {
        format!(
            "{} dage siden seneste backup",
            text(&c["daysSinceLatestBackup"])
        )
    };
    let mut metrics = String::new();
    for (k, v) in [
        ("Status", overall),
        ("Seneste backup", escape(&instant(&c["latestBackupAt"]))),
        ("Dage siden backup", escape(&days)),
        (
            "Backup forfalden",
            if c["backupDue"] == true {
                pill("bad", "ja")
            } else {
                pill("ok", "nej")
            },
        ),
        (
            "Seneste backup på attesteret EU/EØS-destination",
            if g["latestBackupPlacedOffsite"] == true {
                pill("ok", "ja, verificeret")
            } else {
                pill("bad", "nej")
            },
        ),
        ("Bogførings-lås", lock_pill),
    ] {
        metrics.push_str(&format!(
            "<div class=\"metric\"><span class=\"k\">{}</span><span class=\"v\">{v}</span></div>",
            escape(k)
        ));
    }
    let mut table = String::new();
    let destinations = g["destinations"].as_array();
    if destinations.is_none_or(|a| a.is_empty()) {
        table.push_str("<p>Ingen backup-destinationer er konfigureret endnu. Tilføj én med <code>rentemester system backup-add-destination</code> — og husk EU/EØS-kravet nedenfor.</p>");
    } else {
        table.push_str("<table><thead><tr><th>Destination</th><th>Type</th><th>EU/EØS</th><th>Ikke-nærtstående</th><th>§4</th></tr></thead><tbody>");
        for d in destinations.unwrap() {
            let eu = d["regionAttestation"]["inEeaOrEu"] == true;
            let independent = d["nonRelatedParty"] == true;
            let compliant =
                eu && independent && d["itSecurityAttestation"]["meetsRecognisedStandards"] == true;
            let country = d["regionAttestation"]["country"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| format!(" ({})", escape(s)))
                .unwrap_or_default();
            table.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}{country}</td><td>{}</td><td>{}</td></tr>",
                escape(d["label"].as_str().unwrap_or("")),
                escape(d["kind"].as_str().unwrap_or("")),
                yes(eu),
                yes(independent),
                if compliant {
                    pill("ok", "§4 opfyldt")
                } else {
                    pill("bad", "ikke §4")
                }
            ));
        }
        table.push_str("</tbody></table>");
    }
    let warning = if lock["enforced"] == true && lock["locked"] == true {
        format!(
            "<p style=\"color:var(--danger);margin-top:12px;\">{} Kør <code>rentemester system backup</code> for at låse op; placér derefter arkivet på en EU/EØS-destination for at opfylde § 4.</p>",
            escape(lock["reason"].as_str().unwrap_or(""))
        )
    } else {
        String::new()
    };
    let status = format!(
        "<section class=\"card\"><h2>Din status nu</h2><p class=\"sub\">Tjekket pr. {}.</p>{metrics}<h3>Konfigurerede destinationer</h3>{table}{warning}</section>",
        escape(&instant(&g["checkedAt"]))
    );
    include_str!("guide.html")
        .replace("@@NAME@@", &escape(name))
        .replace("@@STATUS@@", &status)
        .replace("@@GENERATED@@", &escape(&instant(&g["checkedAt"])))
}
pub(super) fn write_private(path: &Path, bytes: &[u8], protected: &Path) -> Result<(), String> {
    use std::io::Write;
    if bytes.len() > 1024 * 1024 {
        return Err("guide exceeds1MiB".into());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let canonical = std::fs::canonicalize(parent).map_err(|e| e.to_string())?;
    // Walk actual output ancestors without accepting a symlink alias.
    let absolute = if parent.is_absolute() {
        parent.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(parent)
    };
    let mut seen = std::path::PathBuf::new();
    for component in absolute.components() {
        seen.push(component);
        if std::fs::symlink_metadata(&seen)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("guide output ancestor must not be a symlink".into());
        }
    }
    if canonical.starts_with(protected) {
        return Err(
            "guide output cannot enter the admitted runtime/profile/company namespace".into(),
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::metadata(&canonical).map_err(|e| e.to_string())?;
        if m.mode() & 0o777 != 0o700 {
            return Err("guide output directory must be private (0700)".into());
        }
    }
    let path = canonical.join(path.file_name().ok_or("guide output requires a filename")?);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("private guide output refused: {e}"))?;
    file.write_all(bytes).map_err(|e| e.to_string())
}
