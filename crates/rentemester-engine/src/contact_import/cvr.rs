use super::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Management {
    pub name: String,
    pub role: String,
}
/// Full existing cvr_lookups business snapshot. Contains no provider configuration or credentials.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CvrCompany {
    pub cvr: String,
    pub name: String,
    pub address: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
    pub municipality_code: Option<i64>,
    pub company_form_code: Option<i64>,
    pub company_form_short: Option<String>,
    pub company_form_long: Option<String>,
    pub status: Option<String>,
    pub industry_code: Option<String>,
    pub industry_text: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub start_date: Option<String>,
    pub fiscal_year_start: Option<String>,
    pub fiscal_year_end: Option<String>,
    pub fiscal_year_start_month: Option<u8>,
    pub audit_waived: Option<bool>,
    pub share_capital: Option<serde_json::Number>,
    pub share_capital_currency: Option<String>,
    pub employees: Option<i64>,
    pub advertising_protected: bool,
    pub management: Vec<Management>,
}
fn string(v: &Value) -> Option<String> {
    v.as_str()
        .map(trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
fn scalar(v: &Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        Some(s.into())
    } else if v.is_number() {
        Some(v.to_string())
    } else {
        None
    }
}
fn number(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}
fn current(v: &Value) -> Option<&Value> {
    let items = v.as_array()?;
    items
        .iter()
        .find(|e| e["periode"].get("gyldigTil") == Some(&Value::Null))
        .or_else(|| items.last())
}
fn attr(entity: &Value, kind: &str) -> Option<String> {
    let a = entity["attributter"]
        .as_array()?
        .iter()
        .find(|a| a["type"] == kind)?;
    string(&current(&a["vaerdier"])?["vaerdi"])
}
fn open(v: &Value) -> Option<String> {
    let e = v
        .as_array()?
        .iter()
        .find(|e| e["periode"].get("gyldigTil") == Some(&Value::Null))?;
    string(&e["vaerdi"])
}
fn public(entity: &Value, key: &str) -> Option<String> {
    let visible: Vec<Value> = entity[key]
        .as_array()?
        .iter()
        .filter(|e| e["hemmelig"] != true)
        .cloned()
        .collect();
    string(&current(&Value::Array(visible))?["kontaktoplysning"])
}
impl CvrCompany {
    pub fn map(entity: &Value, cvr: &str) -> Result<Self, Error> {
        if !entity.is_object() || cvr.len() != 8 || !cvr.bytes().all(|b| b.is_ascii_digit()) {
            return Err(refused("ugyldigt CVR-svar"));
        }
        if let Some(actual) = scalar(&entity["cvrNummer"])
            && actual != cvr
        {
            return Err(refused("CVR-svaret tilhører en anden virksomhed"));
        }
        let meta = &entity["virksomhedMetadata"];
        let address = meta
            .get("nyesteBeliggenhedsadresse")
            .filter(|v| !v.is_null())
            .or_else(|| current(&entity["beliggenhedsadresse"]))
            .unwrap_or(&Value::Null);
        let form = meta
            .get("nyesteVirksomhedsform")
            .filter(|v| !v.is_null())
            .or_else(|| current(&entity["virksomhedsform"]))
            .unwrap_or(&Value::Null);
        let industry = meta
            .get("nyesteHovedbranche")
            .filter(|v| !v.is_null())
            .or_else(|| current(&entity["hovedbranche"]))
            .unwrap_or(&Value::Null);
        let mut parts = vec![];
        if let Some(co) = scalar(&address["conavn"]).filter(|s| !s.is_empty()) {
            parts.push(format!("c/o {co}"));
        }
        let mut street = vec![];
        if let Some(v) = scalar(&address["vejnavn"]).filter(|s| !s.is_empty()) {
            street.push(v);
        }
        if let Some(v) = scalar(&address["husnummerFra"]).filter(|s| s != "0" && !s.is_empty()) {
            street.push(format!(
                "{v}{}",
                scalar(&address["bogstavFra"]).unwrap_or_default()
            ));
        }
        if !street.is_empty() {
            parts.push(street.join(" "));
        }
        let mut door = vec![];
        if let Some(v) = scalar(&address["etage"]).filter(|s| !s.is_empty()) {
            door.push(format!("{v}."));
        }
        if let Some(v) = scalar(&address["sidedoer"]).filter(|s| !s.is_empty()) {
            door.push(v);
        }
        if !door.is_empty() {
            parts.push(door.join(" "));
        }
        let fiscal_year_start = attr(entity, "REGNSKABSÅR_START");
        let month = fiscal_year_start
            .as_deref()
            .filter(|s| {
                s.is_ascii() && s.len() == 7 && s.starts_with("--") && s.as_bytes()[4] == b'-'
            })
            .and_then(|s| s[2..4].parse::<u8>().ok())
            .filter(|m| (1..=12).contains(m));
        let mut management = vec![];
        let mut seen = std::collections::BTreeSet::new();
        if let Some(relations) = entity["deltagerRelation"].as_array() {
            for rel in relations {
                let Some(name) =
                    current(&rel["deltager"]["navne"]).and_then(|e| string(&e["navn"]))
                else {
                    continue;
                };
                if let Some(orgs) = rel["organisationer"].as_array() {
                    for org in orgs {
                        if org["hovedtype"] != "LEDELSESORGAN" {
                            continue;
                        }
                        if let Some(members) = org["medlemsData"].as_array() {
                            for member in members {
                                let role = member["attributter"]
                                    .as_array()
                                    .and_then(|a| a.iter().find(|a| a["type"] == "FUNKTION"))
                                    .and_then(|a| open(&a["vaerdier"]));
                                if let Some(role) = role
                                    && seen.insert((name.clone(), role.clone()))
                                {
                                    management.push(Management {
                                        name: name.clone(),
                                        role,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(Self {
            cvr: cvr.into(),
            name: string(&meta["nyesteNavn"]["navn"])
                .or_else(|| current(&entity["navne"]).and_then(|v| string(&v["navn"])))
                .unwrap_or_else(|| "Ukendt".into()),
            address: (!parts.is_empty()).then(|| parts.join(", ")),
            postal_code: scalar(&address["postnummer"]),
            city: string(&address["postdistrikt"]),
            municipality_code: number(&address["kommune"]["kommuneKode"]),
            company_form_code: number(&form["virksomhedsformkode"]),
            company_form_short: string(&form["kortBeskrivelse"]),
            company_form_long: string(&form["langBeskrivelse"]),
            status: string(&meta["sammensatStatus"]).or_else(|| {
                current(&entity["virksomhedsstatus"]).and_then(|v| string(&v["status"]))
            }),
            industry_code: string(&industry["branchekode"]),
            industry_text: string(&industry["branchetekst"]),
            email: public(entity, "elektroniskPost"),
            phone: public(entity, "telefonNummer"),
            website: public(entity, "hjemmeside"),
            start_date: string(&meta["stiftelsesDato"]),
            fiscal_year_start,
            fiscal_year_end: attr(entity, "REGNSKABSÅR_SLUT"),
            fiscal_year_start_month: month,
            audit_waived: attr(entity, "REVISION_FRAVALGT").map(|v| v == "true"),
            share_capital: attr(entity, "KAPITAL")
                .and_then(|s| s.parse::<serde_json::Number>().ok()),
            share_capital_currency: attr(entity, "KAPITALVALUTA"),
            employees: meta["nyesteAarsbeskaeftigelse"]["antalAnsatte"].as_i64(),
            advertising_protected: entity["reklamebeskyttet"] == true,
            management,
        })
    }
    pub fn full_address(&self) -> Option<String> {
        let city = [self.postal_code.clone(), self.city.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let full = [self.address.clone(), (!city.is_empty()).then_some(city)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
        (!full.is_empty()).then_some(full)
    }
}
