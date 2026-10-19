//! One private unresolved parties attempt; no payload persistence or replay.
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
fn directory() -> PathBuf {
    crate::journal_attempt::directory()
}
fn private(path: &Path, directory: bool) -> Result<(), String> {
    let m = fs::symlink_metadata(path)
        .map_err(|_| "Privat leverandørpartstatussstatus kunne ikke åbnes")?;
    let uid = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|_| "Brugeridentitet kunne ikke læses")?;
    if m.file_type().is_symlink()
        || m.is_dir() != directory
        || (!directory && !m.is_file())
        || m.mode() & 0o077 != 0
        || String::from_utf8_lossy(&uid.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            != Some(m.uid())
    {
        return Err(
            "Leverandørpartstatussstatus kræver din egen private mappe0700/fil0600 uden symbolske links."
                .into(),
        );
    }
    let acl = std::process::Command::new("/bin/ls")
        .arg("-lde")
        .arg(path)
        .output()
        .map_err(|_| "Privat adgangskontrol kunne ikke undersøges")?;
    if !acl.status.success()
        || String::from_utf8_lossy(&acl.stdout)
            .lines()
            .skip(1)
            .any(|line| !line.trim().is_empty())
    {
        return Err(
            "Privat leverandørpartstatussstatus må ikke have udvidede adgangslister.".into(),
        );
    }
    Ok(())
}
fn ensure() -> Result<PathBuf, String> {
    let dir = directory();
    if !dir.is_absolute() {
        return Err("Vælg en absolut privat statusmappe".into());
    }
    if !dir.exists() {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(|_| "Privat statusmappe kunne ikke oprettes")?;
    }
    private(&dir, true)?;
    if dir
        .canonicalize()
        .map_err(|_| "Privat statusmappe kunne ikke åbnes")?
        != dir
    {
        return Err("Statusmappen skal være en kanonisk sti".into());
    }
    Ok(dir)
}
pub fn load() -> Result<Option<Value>, String> {
    load_at(&directory())
}
fn load_at(dir: &Path) -> Result<Option<Value>, String> {
    let path = dir.join("pending-parties.json");
    if !path
        .try_exists()
        .map_err(|_| "Leverandørpartstatussstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Leverandørpartstatussstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Leverandørpartstatussstatus overskrider grænsen".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Leverandørpartstatussstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret leverandørpartstatussforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-parties/1"
        && value.as_object().is_some_and(|m| {
            m.len() == 8
                && m.keys().all(|k| {
                    [
                        "format",
                        "origin",
                        "company",
                        "principal",
                        "ledger",
                        "localIdentity",
                        "operation",
                        "partyId",
                    ]
                    .contains(&k.as_str())
                })
        })
        && ["origin", "company", "principal", "ledger", "operation"]
            .iter()
            .all(|k| {
                value[*k]
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.len() < 1024)
            })
        && matches!(
            value["operation"].as_str(),
            Some("create" | "role" | "alias" | "assertion")
        )
        && (value["localIdentity"].is_null() || value["localIdentity"].is_string())
        && (value["partyId"].is_null() || value["partyId"].is_string())
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig leverandørpartstatussstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-parties.json"))
        .map_err(|_| "Et leverandørpartstatussforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Leverandørpartstatussstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Leverandørpartstatussstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Leverandørpartstatussstatus ændrede sig og bevares".into());
    }
    if manual {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "Systemtid kunne ikke læses")?
            .as_nanos();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join(format!("parties-manually-resolved-{suffix}.json")))
            .map_err(|_| "Vidensafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Vidensafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-parties.json"))
        .map_err(|_| "Leverandørpartstatussstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Leverandørpartstatussstatus kunne ikke synkroniseres".into())
}

use rentemester_client::PartyMutation;
use serde_json::json;
pub const KINDS: &[&str] = &[
    "person",
    "organization",
    "public_authority",
    "financial_institution",
];
pub const ROLES: &[&str] = &[
    "issuer",
    "supplier",
    "customer",
    "recipient",
    "payer",
    "payee",
    "processor",
    "acquirer",
    "related_company",
    "establishment",
    "location",
    "payment_descriptor",
    "vendor",
    "owner",
    "adviser",
    "employee",
    "authority",
    "bank",
];
pub const IDENTIFIERS: &[&str] = &["dk_cvr", "eu_vat", "non_eu"];
pub const REVIEW_STATES: &[&str] = &["proposed", "approved", "rejected"];
#[derive(Clone, Default, PartialEq)]
pub struct Selection {
    pub query: String,
    pub cursor: usize,
    pub id: Option<String>,
}
#[derive(Clone)]
pub struct Snapshot {
    pub company: String,
    pub page: Value,
    pub party: Option<Value>,
    pub principal: String,
    pub ledger: String,
}
fn text(v: &Value, k: &str) -> Result<String, String> {
    v[k].as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serveren mangler {k}"))
}
fn party_valid(p: &Value) -> bool {
    p["partyId"].as_str().is_some_and(|v| !v.is_empty())
        && p["kind"].is_string()
        && p["name"].is_string()
        && ["source", "observedAt", "reviewAssertion"]
            .iter()
            .all(|k| p[*k].is_string())
        && ["identifiers", "roles", "history", "aliases", "assertions"]
            .iter()
            .all(|k| p[*k].is_array())
}
impl Snapshot {
    pub fn parse(
        company: &str,
        selection: &Selection,
        page: Value,
        party: Option<Value>,
        me: Value,
        journal: Value,
    ) -> Result<Self, String> {
        if page["ok"] != true
            || me["ok"] != true
            || journal["ok"] != true
            || journal["journal"]["slug"] != company
            || !page["count"].is_u64()
            || !(page["nextCursor"].is_null() || page["nextCursor"].is_u64())
            || page["rows"]
                .as_array()
                .is_none_or(|a| a.iter().any(|p| !party_valid(p)))
        {
            return Err("Serveren bekræftede ikke synlig partside, virksomhed og identitet".into());
        }
        let party = match (party, &selection.id) {
            (Some(root), Some(id))
                if root["ok"] == true
                    && root["party"]["partyId"] == *id
                    && party_valid(&root["party"]) =>
            {
                Some(root["party"].clone())
            }
            (None, None) => None,
            _ => return Err("Den valgte kanoniske part mangler eller matcher ikke".into()),
        };
        Ok(Self {
            company: company.into(),
            page,
            party,
            principal: text(&me["user"], "id")?,
            ledger: text(&journal["journal"], "ledgerUuid")?,
        })
    }
}
pub fn identifier(country: &str, kind: &str, value: &str) -> Result<Value, String> {
    if country.trim().is_empty() || value.trim().is_empty() || !IDENTIFIERS.contains(&kind) {
        return Err("Angiv land, vælg identifikationsart og angiv identifikatoren eksplicit. Serveren normaliserer og validerer.".into());
    }
    Ok(json!({"country":country,"identifierKind":kind,"identifier":value}))
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Role,
    Alias,
    Assertion,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Role => "role",
            Self::Alias => "alias",
            Self::Assertion => "assertion",
        }
    }
}
#[derive(Default)]
pub struct Form<'a> {
    pub kind: &'a str,
    pub name: &'a str,
    pub identifiers: &'a [Value],
    pub role: &'a str,
    pub defaults: &'a str,
    pub source: &'a str,
    pub observed: &'a str,
    pub review_assertion: &'a str,
    pub review_state: &'a str,
    pub alias: &'a str,
    pub field: &'a str,
    pub value: &'a str,
}
#[derive(Clone)]
pub struct Review {
    pub action: Action,
    pub source: Snapshot,
    pub body: Vec<u8>,
}
impl Review {
    pub fn prepare(data: &Snapshot, action: Action, form: Form<'_>) -> Result<Self, String> {
        if action != Action::Create && data.party.is_none() {
            return Err("Vælg og inspicér en faktisk synlig part først".into());
        }
        let body = match action {
            Action::Create | Action::Role => {
                if !ROLES.contains(&form.role) {
                    return Err("Vælg den konkrete rolle i ankervirksomheden eksplicit".into());
                }
                let defaults: Value = serde_json::from_str(form.defaults).map_err(|e| {
                    format!("Standarder kræver eksplicit JSON-objekt ({{}} for ingen): {e}")
                })?;
                if defaults.as_object().is_none_or(|m| {
                    m.keys().any(|k| {
                        !["account", "vat", "currency", "paymentTermsDays"].contains(&k.as_str())
                    })
                }) {
                    return Err("Vælg kun de understøttede standardfelter: account, vat, currency, paymentTermsDays. Ingen standard udledes lokalt.".into());
                }
                if action == Action::Role {
                    json!({"role":form.role,"defaults":defaults,"confirm":true})
                } else {
                    if !KINDS.contains(&form.kind)
                        || [form.name, form.source, form.observed, form.review_assertion]
                            .iter()
                            .any(|s| s.trim().is_empty())
                    {
                        return Err("Angiv art, navn, kilde, observationstid og din konkrete review-erklæring før oprettelse".into());
                    }
                    json!({"kind":form.kind,"name":form.name,"identifiers":form.identifiers,"source":form.source,"observedAt":form.observed,"reviewAssertion":form.review_assertion,"role":form.role,"defaults":defaults,"confirm":true})
                }
            }
            Action::Alias | Action::Assertion => {
                if !REVIEW_STATES.contains(&form.review_state)
                    || [form.source, form.observed]
                        .iter()
                        .any(|s| s.trim().is_empty())
                {
                    return Err("Angiv kilde, observationstid og eksplicit reviewtilstand".into());
                }
                if action == Action::Alias {
                    if form.alias.trim().is_empty() {
                        return Err("Angiv det kildebelagte alias".into());
                    }
                    json!({"alias":form.alias,"source":form.source,"observedAt":form.observed,"reviewState":form.review_state,"confirm":true})
                } else {
                    if form.field.trim().is_empty() || form.value.trim().is_empty() {
                        return Err("Angiv assertionens felt og tekstværdi; kanonisk identitet overskrives ikke".into());
                    }
                    json!({"field":form.field,"value":form.value,"source":form.source,"observedAt":form.observed,"reviewState":form.review_state,"confirm":true})
                }
            }
        };
        let body = serde_json::to_vec(&body).map_err(|_| "Anmodningen kunne ikke fastholdes")?;
        if body.len() > rentemester_client::MAX_PARTY_BODY {
            return Err("Anmodningen overskrider transportgrænsen".into());
        }
        Ok(Self {
            action,
            source: data.clone(),
            body,
        })
    }
    pub fn current(&self, data: &Snapshot) -> bool {
        self.source.company == data.company
            && self.source.principal == data.principal
            && self.source.ledger == data.ledger
            && self.source.page == data.page
            && self.source.party == data.party
    }
    pub fn sdk(&self) -> Result<PartyMutation, String> {
        if self.action == Action::Create {
            return Ok(PartyMutation::Create);
        }
        let id = text(self.source.party.as_ref().ok_or("Part mangler")?, "partyId")?;
        Ok(match self.action {
            Action::Role => PartyMutation::Role(id),
            Action::Alias => PartyMutation::Alias(id),
            Action::Assertion => PartyMutation::Assertion(id),
            _ => return Err("Ukendt handling".into()),
        })
    }
    pub fn marker(&self, origin: &str, local: Option<&str>) -> Value {
        json!({"format":"rentemester-desktop-parties/1","origin":origin,"company":self.source.company,"principal":self.source.principal,"ledger":self.source.ledger,"localIdentity":local,"operation":self.action.name(),"partyId":if self.action==Action::Create{Value::Null}else{self.source.party.as_ref().map(|p|p["partyId"].clone()).unwrap_or(Value::Null)}})
    }
    pub fn receipt(&self, value: &Value) -> Option<Value> {
        let p = &value["party"];
        if value["ok"] != true || !party_valid(p) {
            return None;
        }
        let body: Value = serde_json::from_slice(&self.body).ok()?;
        if self.action == Action::Create {
            if p["kind"] != body["kind"]
                || p["name"] != body["name"]
                || p["source"] != body["source"]
                || p["reviewAssertion"] != body["reviewAssertion"]
            {
                return None;
            }
        } else {
            let old = self.source.party.as_ref()?;
            if [
                "partyId",
                "kind",
                "name",
                "identifiers",
                "source",
                "observedAt",
                "reviewAssertion",
            ]
            .iter()
            .any(|k| p[*k] != old[*k])
            {
                return None;
            }
        }
        if matches!(self.action, Action::Create | Action::Role)
            && p["roles"].as_array()?.iter().all(|r| {
                r["companySlug"] != self.source.company
                    || r["role"] != body["role"]
                    || r["defaults"] != body["defaults"]
            })
        {
            return None;
        }
        if self.action == Action::Alias
            && p["aliases"]
                .as_array()?
                .iter()
                .all(|r| r["source"] != body["source"] || r["review_state"] != body["reviewState"])
        {
            return None;
        }
        if self.action == Action::Assertion
            && p["assertions"].as_array()?.iter().all(|r| {
                r["field"] != body["field"]
                    || r["value"] != body["value"]
                    || r["source"] != body["source"]
                    || r["review_state"] != body["reviewState"]
            })
        {
            return None;
        }
        Some(p.clone())
    }
}
pub fn readback(receipt: &Value, data: &Snapshot) -> bool {
    data.party.as_ref() == Some(receipt)
}
pub fn scope(record: &Value, origin: &str, data: &Snapshot, local: Option<&str>) -> bool {
    record["company"] == data.company
        && record["principal"] == data.principal
        && record["ledger"] == data.ledger
        && record["localIdentity"].as_str() == local
        && (record["origin"] == origin || local.is_some())
}
pub fn lines(v: &Value, prefix: &str) -> Vec<String> {
    match v {
        Value::Object(m) => m
            .iter()
            .flat_map(|(k, v)| lines(v, &format!("{prefix}.{k}")))
            .collect(),
        Value::Array(a) => {
            if a.is_empty() {
                vec![format!("{prefix}: []")]
            } else {
                a.iter()
                    .enumerate()
                    .flat_map(|(i, v)| lines(v, &format!("{prefix}[{i}]")))
                    .collect()
            }
        }
        Value::String(s) => vec![format!("{prefix}: {s}")],
        _ => vec![format!("{prefix}: {v}")],
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn party() -> Value {
        json!({"partyId":"synthetic-party","kind":"organization","name":"Synthetic services","source":"synthetic owner declaration","observedAt":"2026-01-01T00:00:00.000Z","reviewAssertion":"reviewed original source","identifiers":[{"country":"DK","identifierKind":"dk_cvr","identifier":"DK12345678"}],"aliases":[],"assertions":[],"roles":[{"companySlug":"synthetic","role":"supplier","defaults":{}}],"history":[{"event_type":"created","payload_hash":"a".repeat(64),"actor":"user:owner","created_at":"2026-01-01T00:00:00.000Z"}]})
    }
    fn snapshot() -> Snapshot {
        Snapshot::parse(
            "synthetic",
            &Selection {
                id: Some("synthetic-party".into()),
                ..Selection::default()
            },
            json!({"ok":true,"rows":[party()],"count":26,"nextCursor":25}),
            Some(json!({"ok":true,"party":party()})),
            json!({"ok":true,"user":{"id":"owner"}}),
            json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"immutable-ledger"}}),
        )
        .unwrap()
    }
    fn form() -> Form<'static> {
        Form {
            kind: "organization",
            name: "Synthetic services",
            identifiers: &[],
            role: "supplier",
            defaults: "{}",
            source: "synthetic owner declaration",
            observed: "2026-01-01",
            review_assertion: "reviewed original source",
            review_state: "approved",
            alias: "synthetic alias",
            field: "email",
            value: "synthetic@example.invalid",
        }
    }
    #[test]
    fn typed_identifiers_and_explicit_defaults_are_frozen_without_inference() {
        let id = identifier(" de ", "eu_vat", "de123456789").unwrap();
        assert_eq!(id["country"], " de ");
        assert_eq!(id["identifier"], "de123456789");
        assert!(identifier("", "eu_vat", "123").is_err());
        assert!(identifier("DK", "guessed", "123").is_err());
        let ids = vec![id];
        let review=Review::prepare(&snapshot(),Action::Create,Form{identifiers:&ids,defaults:r#"{"account":"3000","vat":{"choice":"explicit"},"currency":"DKK","paymentTermsDays":14}"#,..form()}).unwrap();
        let body: Value = serde_json::from_slice(&review.body).unwrap();
        assert_eq!(body["identifiers"], json!(ids));
        assert_eq!(body["defaults"]["paymentTermsDays"], 14);
        assert!(body.get("partyId").is_none());
        assert!(body.get("idempotencyKey").is_none());
        assert_eq!(body["observedAt"], "2026-01-01");
        for defaults in ["", "[]", r#"{"inventedDefault":true}"#] {
            assert!(
                Review::prepare(&snapshot(), Action::Create, Form { defaults, ..form() }).is_err()
            );
        }
        assert!(Review::prepare(&snapshot(), Action::Create, Form { role: "", ..form() }).is_err());
    }
    #[test]
    fn pagination_identity_and_provenance_are_canonical_not_locally_filtered() {
        let data = snapshot();
        assert_eq!(data.page["count"], 26);
        let mut missing = party();
        missing.as_object_mut().unwrap().remove("source");
        assert!(!party_valid(&missing));
        assert_eq!(data.page["nextCursor"], 25);
        let all = lines(data.party.as_ref().unwrap(), "part");
        for key in ["identifiers", "roles", "source", "history", "payload_hash"] {
            assert!(all.iter().any(|s| s.contains(key)));
        }
        assert!(
            Snapshot::parse(
                "synthetic",
                &Selection::default(),
                json!({"ok":true,"rows":[],"count":0,"nextCursor":null}),
                None,
                json!({"ok":true,"user":{"id":"owner"}}),
                json!({"ok":true,"journal":{"slug":"other","ledgerUuid":"immutable"}})
            )
            .is_err()
        );
    }
    #[test]
    fn source_and_scope_changes_block_frozen_requests() {
        let data = snapshot();
        let review = Review::prepare(&data, Action::Alias, form()).unwrap();
        assert!(review.current(&data));
        for key in ["principal", "ledger", "company", "page", "party"] {
            let mut changed = data.clone();
            match key {
                "principal" => changed.principal = "other".into(),
                "ledger" => changed.ledger = "other".into(),
                "company" => changed.company = "other".into(),
                "page" => changed.page["count"] = json!(27),
                _ => changed.party.as_mut().unwrap()["source"] = json!("changed"),
            };
            assert!(!review.current(&changed));
        }
        let mut no_target = data;
        no_target.party = None;
        assert!(Review::prepare(&no_target, Action::Role, form()).is_err());
        assert!(Review::prepare(&no_target, Action::Create, form()).is_ok());
    }
    #[test]
    fn creation_role_and_assertion_receipts_preserve_immutable_identity_and_require_exact_readback()
    {
        let data = snapshot();
        let create = Review::prepare(&data, Action::Create, form()).unwrap();
        let result = json!({"ok":true,"party":party()});
        let receipt = create.receipt(&result).unwrap();
        assert!(readback(&receipt, &data));
        let mut changed = data.clone();
        changed.party.as_mut().unwrap()["name"] = json!("changed");
        assert!(!readback(&receipt, &changed));
        let role = Review::prepare(&data, Action::Role, form()).unwrap();
        assert!(matches!(role.sdk().unwrap(),PartyMutation::Role(id)if id=="synthetic-party"));
        assert!(role.receipt(&result).is_some());
        let mut wrong = result.clone();
        wrong["party"]["partyId"] = json!("other");
        assert!(role.receipt(&wrong).is_none());
        wrong = result.clone();
        wrong["party"]["roles"][0]["defaults"] = json!({"account":"different"});
        assert!(role.receipt(&wrong).is_none());
        let assertion = Review::prepare(&data, Action::Assertion, form()).unwrap();
        let mut result = result;
        result["party"]["assertions"] = json!([{"field":"email","value":"synthetic@example.invalid","source":"synthetic owner declaration","observed_at":"2026-01-01T00:00:00.000Z","review_state":"approved","payload_hash":"b".repeat(64)}]);
        assert!(assertion.receipt(&result).is_some());
        result["party"]["name"] = json!("overwritten identity");
        assert!(assertion.receipt(&result).is_none());
    }
    #[test]
    fn source_backed_alias_and_field_wire_is_explicit_and_not_an_identity_update() {
        let data = snapshot();
        for action in [Action::Alias, Action::Assertion] {
            let review = Review::prepare(&data, action, form()).unwrap();
            let body: Value = serde_json::from_slice(&review.body).unwrap();
            assert_eq!(body["reviewState"], "approved");
            assert_eq!(body["source"], "synthetic owner declaration");
            assert_eq!(body["observedAt"], "2026-01-01");
            assert!(body.get("name").is_none());
            assert!(body.get("identifiers").is_none());
            assert!(
                Review::prepare(
                    &data,
                    action,
                    Form {
                        review_state: "",
                        ..form()
                    }
                )
                .is_err()
            );
            assert!(
                Review::prepare(
                    &data,
                    action,
                    Form {
                        source: "",
                        ..form()
                    }
                )
                .is_err()
            );
        }
        let alias = Review::prepare(&data, Action::Alias, form()).unwrap();
        let mut p = party();
        p["aliases"] = json!([{"alias":"synthetic alias","source":"synthetic owner declaration","review_state":"approved","observed_at":"2026-01-01T00:00:00.000Z","payload_hash":"b".repeat(64)}]);
        assert!(alias.receipt(&json!({"ok":true,"party":p})).is_some());
    }
    #[test]
    fn unknown_marker_excludes_personal_payload_and_requires_original_scope() {
        let data = snapshot();
        let review = Review::prepare(&data, Action::Create, form()).unwrap();
        let record = review.marker("https://synthetic.invalid", None);
        assert!(valid_record(&record));
        assert_eq!(record["partyId"], Value::Null);
        for field in [
            "name",
            "identifiers",
            "defaults",
            "query",
            "source",
            "body",
            "value",
        ] {
            let mut invalid = record.clone();
            invalid[field] = json!("not persisted");
            assert!(!valid_record(&invalid));
        }
        assert!(scope(&record, "https://synthetic.invalid", &data, None));
        assert!(!scope(&record, "https://other.invalid", &data, None));
        let local = review.marker("http://127.0.0.1:10001", Some("owned-profile"));
        assert!(scope(
            &local,
            "http://127.0.0.1:10002",
            &data,
            Some("owned-profile")
        ));
        assert!(!scope(
            &local,
            "http://127.0.0.1:10002",
            &data,
            Some("other-profile")
        ));
    }
    #[test]
    fn pending_blocks_replay_survives_reload_and_manual_archive_is_explicit() {
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-native-parties-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let record = Review::prepare(&snapshot(), Action::Alias, form())
            .unwrap()
            .marker("https://synthetic.invalid", None);
        begin_at(&dir, &record).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(record.clone()));
        assert!(begin_at(&dir, &record).is_err());
        assert!(finish_at(&dir, &json!({"wrong":"record"}), false).is_err());
        finish_at(&dir, &record, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        assert!(fs::read_dir(&dir).unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("parties-manually-resolved-")
        }));
        fs::remove_dir_all(dir).unwrap();
    }
}
