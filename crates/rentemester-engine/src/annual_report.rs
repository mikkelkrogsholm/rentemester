//! Class-B preparation metadata. No filing, approval, or statutory completeness claim.
use crate::{Error, fiscal_year::Date};
use serde_json::{Value, json};
pub fn prior_date(date: &str) -> Result<String, Error> {
    Date::parse(date)?;
    let year: u16 = date[..4].parse().unwrap();
    let year = year
        .checked_sub(1)
        .ok_or_else(|| Error::new("invalid_date", "prior year outside supported range"))?;
    let suffix = if &date[4..] == "-02-29" {
        "-02-28"
    } else {
        &date[4..]
    };
    Ok(format!("{year:04}{suffix}"))
}
pub fn placeholders(end: &str) -> Value {
    json!({"notes":[
        {"id":"accounting-policies","title":"Anvendt regnskabspraksis","body":"Årsrapporten forberedes efter regnskabsklasse B. Udfyldes og gennemgås af ejer eller revisor.","placeholder":true},
        {"id":"staff-costs","title":"Personaleomkostninger","body":"Note-skelet. Udfyldes af ejer eller revisor.","placeholder":true},
        {"id":"average-employees","title":"Gennemsnitligt antal beskæftigede","body":"Oplys gennemsnitligt antal beskæftigede. Rentemester har ingen løndata — udfyldes af ejer eller revisor.","placeholder":true},
        {"id":"equity","title":"Egenkapital","body":"Note-skelet for egenkapitalens bevægelser. Udfyldes af ejer eller revisor.","placeholder":true},
        {"id":"subsequent-events","title":"Begivenheder efter balancedagen","body":"Note-skelet. Udfyldes af ejer eller revisor.","placeholder":true}],
        "ledelsespategning":{"placeholder":true,"text":format!("Ledelsen har dags dato behandlet og godkendt årsrapporten for regnskabsåret, der slutter {end}. Dette er en skabelon — ledelsen indsætter dato, sted og underskrifter inden indberetning.")}})
}
#[cfg(test)]
mod tests {
    #[test]
    fn prior_dates_are_calendar_based() {
        assert_eq!(super::prior_date("2024-02-29").unwrap(), "2023-02-28");
        assert_eq!(super::prior_date("2026-07-01").unwrap(), "2025-07-01");
        assert!(super::prior_date("0000-01-01").is_err());
        assert!(super::prior_date("2025-02-29").is_err());
    }
}
