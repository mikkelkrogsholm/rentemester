use serde_json::Value;
fn label(family:&str)->&str {
    match family {
        "retention"=>"Opbevaringsstatus",
        "gdpr"=>"Samler alle persondata Rentemester har om en kunde/leverandør i én indsigtsrapport med opbevaringsvurdering. Append-only audit-logges og er derfor actor-gatet.",
        "gdpr-erase"=>"Legacy navn for gdpr forget — sletter/redigerer persondata der ikke længere er under bogføringsmæssig opbevaringspligt. Append-only ledger og audit-kæde røres aldrig.",
        "opening-balance"=>"Bogfører virksomhedens primobalance som én balanceret, audited åbningspostering pr. en skæringsdato. Idempotent: præcis én primobalance pr. virksomhed.",
        _=>"Compliance-rapport",
    }
}
pub(super) fn render(v:&Value,family:&str)->String {
    if v["ok"]==false {
        let mut lines=vec![format!("✘ {} failed",label(family))];
        for e in v["errors"].as_array().into_iter().flatten(){if let Some(s)=e.as_str(){lines.push(format!("  → {s}"));}}
        return format!("{}\n",lines.join("\n"));
    }
    if family=="retention" {
        let mut lines=vec![format!("Opbevaringsstatus pr. {}",v["asOf"].as_str().unwrap_or("—")),String::new()];
        let rows=v["rows"].as_array();
        if rows.is_none_or(|a|a.is_empty()){lines.push("Ingen materiale registreret.".into());}
        else {for r in rows.into_iter().flatten(){
            let table=r["table"].as_str().unwrap_or("—");
            let name=match table{"documents"=>"Bilag","journal_entries"=>"Finansposteringer","bank_transactions"=>"Banktransaktioner",_=>table};
            lines.push(format!("{name}:"));
            lines.push(format!("  I alt: {} | Udløbet: {}",r["total"],r["expired"]));
            if let Some(s)=r["nextExpiry"].as_str(){lines.push(format!("  Næste udløb: {s}"));}
            if let Some(s)=r["oldestExpired"].as_str(){lines.push(format!("  Ældste udløbne: {s}"));}
            lines.push(String::new());
        } if lines.last().is_some_and(String::is_empty){lines.pop();}}
        return format!("{}\n",lines.join("\n"));
    }
    let full=label(family);
    let first=full.split(" — ").next().unwrap_or(full).split(". ").next().unwrap_or(full);
    let heading=if first.chars().count()>60 {format!("{}…",first.chars().take(57).collect::<String>().trim_end())} else{first.to_string()};
    let mut lines=vec![format!("✔ {heading}")];
    for (key,name) in [("entryNo","Posteringsnummer"),("erasedCount","Anonymiseret"),("skippedCount","Sprunget over"),("alreadyErasedCount","Allerede anonymiseret"),("recordCount","Rækker fundet"),("count","Antal"),("total","I alt")] {
        let value=&v[key];
        if let Some(s)=value.as_str().map(str::to_owned).or_else(||value.is_number().then(||value.to_string())){lines.push(format!("  {name}: {s}"));}
    }
    format!("{}\n",lines.join("\n"))
}
