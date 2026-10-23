use super::*;
use roxmltree::Node;
fn named(n: Node<'_, '_>, s: &str) -> bool {
    n.is_element() && n.tag_name().name() == s
}
fn direct_text(n: Node<'_, '_>) -> String {
    n.children()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect()
}
fn chars(n: Node<'_, '_>, skip: &[roxmltree::NodeId], parts: &mut Vec<String>) {
    if skip.contains(&n.id()) {
        return;
    }
    if named(n, "Char") {
        let t = direct_text(n);
        if !t.is_empty() {
            parts.push(t);
        }
        return;
    }
    for c in n.children() {
        chars(c, skip, parts);
    }
}
fn text(n: Node<'_, '_>, skip: &[roxmltree::NodeId]) -> String {
    let mut parts = vec![];
    chars(n, skip, &mut parts);
    parts
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn ordinal(s: &str) -> Option<String> {
    let digits: String = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if !digits.is_empty() {
        Some(digits)
    } else {
        s.chars()
            .find(|c| c.is_alphabetic())
            .map(|c| c.to_lowercase().to_string())
    }
}
fn kind(s: &str, commencement: bool, amendment: bool) -> &'static str {
    let lower = s.to_lowercase();
    let starts = lower.find("træder i kraft").is_some_and(|i| {
        i <= 30
            || lower[..i]
                .replace("og", "")
                .chars()
                .all(|c| c.is_ascii_digit() || c.is_whitespace() || "§.,()-–".contains(c))
    });
    if commencement || starts {
        "commencement"
    } else if amendment {
        "amendment"
    } else {
        "operative"
    }
}
pub fn extract(xml: &str, source: &str) -> Result<Vec<Value>, Error> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| invalid(&format!("malformed XML {source}: {e}")))?;
    let root = doc.root();
    let content = root
        .descendants()
        .find(|n| named(*n, "DokumentIndhold"))
        .ok_or_else(|| invalid("XML missing DokumentIndhold"))?;
    let amendment = root.descendants().any(|n| {
        (named(n, "DocumentType") && direct_text(n).contains('Æ'))
            || (named(n, "DocumentTitle") && direct_text(n).to_lowercase().contains("ændring af"))
    });
    let mut out = vec![];
    for p in content.descendants().filter(|n| {
        n.is_element()
            && [
                "Paragraf",
                "AendringCentreretParagraf",
                "IkraftCentreretParagraf",
            ]
            .contains(&n.tag_name().name())
    }) {
        let Some(id) = p.attribute("localId") else {
            continue;
        };
        let amend = amendment
            || named(p, "AendringCentreretParagraf")
            || p.descendants()
                .any(|n| named(n, "Aendring") || named(n, "AendringDefinition"));
        let commence = named(p, "IkraftCentreretParagraf");
        let mut stks = p
            .children()
            .filter(|n| named(*n, "Stk"))
            .collect::<Vec<_>>();
        if stks.is_empty() {
            stks.push(p);
        }
        for (i, stk) in stks.into_iter().enumerate() {
            let exp = stk.children().find(|n| named(*n, "Explicatus"));
            let ord = exp
                .and_then(|e| {
                    let s = direct_text(e);
                    if s.to_lowercase().contains("stk.") {
                        ordinal(&s)
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| (i + 1).to_string());
            let indents = stk
                .descendants()
                .filter(|n| named(*n, "Index"))
                .flat_map(|n| n.children().filter(|c| named(*c, "Indentatio")))
                .collect::<Vec<_>>();
            let mut skip = indents.iter().map(Node::id).collect::<Vec<_>>();
            skip.extend(
                stk.children()
                    .filter(|n| named(*n, "Explicatus"))
                    .map(|n| n.id()),
            );
            let t = text(stk, &skip);
            out.push(json!({"sourceId":source,"ref":format!("§ {id}, stk. {ord}"),"path":[id,ord],"kind":kind(&t,commence,amend),"textHash":format!("sha256:{}",sha(&t)),"text":t}));
            for (j, indent) in indents.iter().enumerate() {
                let exp = indent.children().find(|n| named(*n, "Explicatus"));
                let o = exp
                    .and_then(|e| ordinal(&direct_text(e)))
                    .unwrap_or_else(|| (j + 1).to_string());
                let skip = indent
                    .descendants()
                    .filter(|n| named(*n, "Index"))
                    .chain(exp)
                    .map(|n| n.id())
                    .collect::<Vec<_>>();
                let t = text(*indent, &skip);
                let label = if indent.attribute("formaInd") == Some("Litra") {
                    "litra"
                } else {
                    "nr."
                };
                out.push(json!({"sourceId":source,"ref":format!("§ {id}, stk. {ord}, {label} {o}"),"path":[id,ord,o],"kind":kind(&t,commence,amend),"textHash":format!("sha256:{}",sha(&t)),"text":t}));
            }
        }
    }
    out.sort_by_cached_key(|p| {
        (
            p["path"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| parsers::id_key(v.as_str().unwrap()))
                .collect::<Vec<_>>(),
            p["ref"].as_str().unwrap().to_owned(),
        )
    });
    Ok(out)
}
pub fn find<'a>(provisions: &'a [Value], r: &str) -> Option<&'a Value> {
    let path = parsers::reference(r)?;
    provisions.iter().find(|p| p["path"] == json!(path))
}
