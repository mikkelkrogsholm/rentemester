//! Read-only recovery against the complete, license-scoped official listings.
//! Source: https://api.digisense.dk/ap/api/rest/openapi-spec.json (2026-10-20).
//! Neither listing documents pagination. Unknown outer fields therefore refuse
//! rather than treating a partial page as absence and attempting registration.
use super::*;
use std::collections::BTreeSet;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompanyType {
    #[serde(rename = "type")]
    kind: String,
    id: String,
}
impl CompanyType {
    fn valid(&self) -> bool {
        match self.kind.as_str() {
            "DK:CVR" => {
                cvr(&self.id)
                    || self
                        .id
                        .strip_prefix("DKFO")
                        .is_some_and(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_digit()))
            }
            "DE:VAT" => self
                .id
                .strip_prefix("DE")
                .is_some_and(|s| s.len() == 9 && s.bytes().all(|b| b.is_ascii_digit())),
            "NIP" | "SE:ORGNR" => {
                self.id.len() == 10 && self.id.bytes().all(|b| b.is_ascii_digit())
            }
            "NO:ORG" => self.id.len() == 9 && self.id.bytes().all(|b| b.is_ascii_digit()),
            _ => false,
        }
    }
    fn matches(&self, cvr_id: &str) -> bool {
        self.kind == "DK:CVR" && self.id == cvr_id
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Company {
    company_type: CompanyType,
    company_name: String,
    created_at: String,
    company_key: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Companies {
    api_license_key: String,
    company_key_constraint: Option<String>,
    companies: Vec<Company>,
}
fn date(s: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(s).is_ok()
}
fn license(s: &str) -> bool {
    text(s, 4096)
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}
fn company(
    bytes: &[u8],
    cvr_id: &str,
    name: &str,
    secret: &str,
) -> Result<Option<CompanyRegistration>, Error> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| Error::Protocol)?;
    if !value.as_object().is_some_and(|v| {
        ["apiLicenseKey", "companyKeyConstraint", "companies"]
            .iter()
            .all(|k| v.contains_key(*k))
    }) {
        return Err(Error::Protocol);
    }
    let v: Companies = serde_json::from_value(value).map_err(|_| Error::Protocol)?;
    if !license(&v.api_license_key) || v.companies.len() > 4096 {
        return Err(Error::Protocol);
    }
    if let Some(key) = &v.company_key_constraint {
        if !company_key(key) || v.companies.len() != 1 || v.companies[0].company_key != *key {
            return Err(Error::Protocol);
        }
    }
    let mut keys = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut found = None;
    for c in v.companies {
        if !c.company_type.valid()
            || !text(&c.company_name, 4096)
            || !date(&c.created_at)
            || !company_key(&c.company_key)
            || c.company_key.contains(secret)
            || !keys.insert(c.company_key.clone())
            || !identities.insert((c.company_type.kind.clone(), c.company_type.id.clone()))
        {
            return Err(Error::Protocol);
        }
        if c.company_type.matches(cvr_id) {
            if c.company_name != name || found.is_some() {
                return Err(Error::Protocol);
            }
            found = Some(CompanyRegistration {
                company_key: c.company_key,
            });
        }
    }
    Ok(found)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompanyInfo {
    company_type: CompanyType,
    company_name: String,
    created_at: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Participants {
    api_license_key: String,
    company_key: Option<String>,
    company: Option<CompanyInfo>,
    participants: Vec<Participant>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Participant {
    network_type: String,
    company_key: String,
    direction: String,
    participant_type: String,
    participant_id: String,
    webhook_url: Option<String>,
    #[serde(default)]
    document_profiles: Option<String>,
    #[serde(default)]
    network_mode: Option<String>,
    created_at: String,
}
fn outbound(
    bytes: &[u8],
    key: &str,
    cvr_id: &str,
    name: &str,
    secret: &str,
) -> Result<Option<ParticipantRegistration>, Error> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| Error::Protocol)?;
    if !value.as_object().is_some_and(|v| {
        ["apiLicenseKey", "companyKey", "company", "participants"]
            .iter()
            .all(|k| v.contains_key(*k))
    }) {
        return Err(Error::Protocol);
    }
    if !value["participants"].as_array().is_some_and(|ps| {
        ps.iter().all(|p| {
            p.get("webhookUrl").is_some()
                && (p["networkType"] == "ksef" || p.get("documentProfiles").is_some())
        })
    }) {
        return Err(Error::Protocol);
    }
    let v: Participants = serde_json::from_value(value).map_err(|_| Error::Protocol)?;
    let c = v.company.ok_or(Error::Protocol)?;
    if !license(&v.api_license_key)
        || v.company_key.as_deref() != Some(key)
        || !c.company_type.matches(cvr_id)
        || c.company_name != name
        || !date(&c.created_at)
        || v.participants.len() > 4096
    {
        return Err(Error::Protocol);
    }
    let mut seen = BTreeSet::new();
    let mut found = false;
    for p in v.participants {
        if p.document_profiles.as_ref().is_some_and(|v| !text(v, 512))
            || p.company_key != key
            || !text(&p.participant_type, 256)
            || !text(&p.participant_id, 512)
            || !date(&p.created_at)
            || !matches!(p.direction.as_str(), "inbound" | "outbound")
            || !seen.insert((
                p.network_type.clone(),
                p.direction.clone(),
                p.participant_type.clone(),
                p.participant_id.clone(),
            ))
        {
            return Err(Error::Protocol);
        }
        if let Some(url) = &p.webhook_url {
            if !text(url, 4096)
                || url.contains(secret)
                || !Url::parse(url).is_ok_and(|u| {
                    u.scheme() == "https"
                        && u.has_host()
                        && u.username().is_empty()
                        && u.password().is_none()
                })
            {
                return Err(Error::Protocol);
            }
        }
        match p.network_type.as_str() {
            "nemhandel" | "peppol" => {
                if p.network_mode.is_some() {
                    return Err(Error::Protocol);
                }
            }
            "ksef" => {
                if !p
                    .network_mode
                    .as_deref()
                    .is_some_and(|m| matches!(m, "PROD" | "DEMO" | "TEST"))
                    || p.document_profiles.is_some()
                {
                    return Err(Error::Protocol);
                }
            }
            _ => return Err(Error::Protocol),
        }
        if p.network_type == "nemhandel" && p.direction == "outbound" {
            // A conflicting participant is not absence; never silently POST over it.
            if found
                || p.participant_type != "DK:CVR"
                || p.participant_id != cvr_id
                || p.webhook_url.is_some()
                || p.document_profiles.as_deref() != Some("default-nemhandel")
            {
                return Err(Error::Protocol);
            }
            found = true;
        }
    }
    Ok(found.then_some(ParticipantRegistration {
        registered_on_network: true,
        webhook_registered: false,
    }))
}
impl Client {
    /// Complete license listing; an error never authorizes a registration POST.
    pub fn registered_company(
        &self,
        cvr_id: &str,
        name: &str,
    ) -> Result<Option<CompanyRegistration>, Error> {
        if !cvr(cvr_id) || !text(name, 4096) {
            return Err(Error::InvalidInput);
        }
        let (status, bytes) = self.request(self.url(&["list-registered-companies"])?, None)?;
        if status != 200 {
            return Err(Error::Refused { status });
        }
        company(&bytes, cvr_id, name, self.secret.as_str())
    }
    /// Existing outbound consent is reusable only for the exact legal company,
    /// Nemhandel profile, participant and null-webhook receive policy.
    pub fn registered_outbound(
        &self,
        key: &str,
        cvr_id: &str,
        name: &str,
    ) -> Result<Option<ParticipantRegistration>, Error> {
        if !company_key(key) || !cvr(cvr_id) || !text(name, 4096) {
            return Err(Error::InvalidInput);
        }
        let mut url = self.url(&["list-participants-for-company"])?;
        url.query_pairs_mut().append_pair("companyKey", key);
        let (status, bytes) = self.request(url, None)?;
        if status != 200 {
            return Err(Error::Refused { status });
        }
        outbound(&bytes, key, cvr_id, name, self.secret.as_str())
    }
}
#[cfg(test)]
mod tests;
