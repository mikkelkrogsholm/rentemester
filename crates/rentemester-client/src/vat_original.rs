//! Source VAT readers. Validation checks projections, never calculates tax locally.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::Value;
fn n(v: &Value, k: &str) -> f64 {
    v[k].as_f64().unwrap_or(f64::NAN)
}
fn eq(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.005
}
fn strings(v: &Value) -> bool {
    v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
}
fn date(v: &Value) -> bool {
    v.as_str().is_some_and(crate::valid_commitment_start)
}
const MONEY: &[&str] = &[
    "outputVat",
    "inputVat",
    "netVatPayable",
    "purchaseBase25",
    "salesBase25",
    "reverseChargeSalesBase",
    "foreignReverseChargeSalesBase",
    "domesticReverseChargeSalesBase",
    "reverseChargePurchaseBase",
    "euGoodsAcquisitionPurchaseBase",
    "euGoodsAcquisitionOutputVat",
    "nonEuServiceReverseChargePurchaseBase",
    "reverseChargePurchaseOutputVat",
    "representationPurchaseBase",
    "badDebtReliefBase25",
    "exemptSalesBase",
    "ossConsumerSalesBase",
];
const RUBRICS: &[&str] = &[
    "salgsmoms",
    "momsAfVarekobUdland",
    "momsAfYdelseskobUdland",
    "kobsmoms",
    "momstilsvar",
    "rubrikA",
    "rubrikB",
    "rubrikC",
];
fn report(v: &Value) -> bool {
    if !["vatPeriodType", "filingDeadline"]
        .iter()
        .all(|k| v.get(k).is_some())
    {
        return false;
    }
    if !MONEY.iter().all(|k| v[k].is_number())
        || !RUBRICS.iter().all(|k| v["rubrikker"][k].is_number())
    {
        return false;
    }
    if !v["ossConsumerSalesEntryCount"].is_u64() {
        return false;
    }
    for (parts, total) in [
        (
            [
                "journalEntryCount",
                "reversedJournalEntryCount",
                "reversalJournalEntryCount",
            ],
            "totalJournalEntryCount",
        ),
        (
            [
                "linesConsidered",
                "reversedLinesConsidered",
                "reversalLinesConsidered",
            ],
            "totalLinesConsidered",
        ),
    ] {
        let Some(t) = v[total].as_u64() else {
            return false;
        };
        let mut sum = 0u64;
        for k in parts {
            let Some(x) = v[k].as_u64() else { return false };
            let Some(s) = sum.checked_add(x) else {
                return false;
            };
            sum = s;
        }
        if sum != t {
            return false;
        }
    }
    if !eq(
        n(&v["rubrikker"], "momsAfVarekobUdland"),
        if n(v, "euGoodsAcquisitionPurchaseBase") > 0. {
            n(v, "euGoodsAcquisitionOutputVat")
        } else {
            0.
        },
    ) {
        return false;
    }
    (v["vatPeriodType"].is_null() || v["vatPeriodType"].is_string())
        && (v["filingDeadline"].is_null() || date(&v["filingDeadline"]))
        && eq(n(v, "outputVat") - n(v, "inputVat"), n(v, "netVatPayable"))
        && eq(
            n(&v["rubrikker"], "salgsmoms") + n(v, "reverseChargePurchaseOutputVat"),
            n(v, "outputVat"),
        )
        && eq(n(&v["rubrikker"], "kobsmoms"), n(v, "inputVat"))
        && eq(n(&v["rubrikker"], "momstilsvar"), n(v, "netVatPayable"))
        && eq(
            n(&v["rubrikker"], "momsAfVarekobUdland")
                + n(&v["rubrikker"], "momsAfYdelseskobUdland"),
            n(v, "reverseChargePurchaseOutputVat"),
        )
        && eq(
            n(&v["rubrikker"], "rubrikA"),
            n(v, "reverseChargePurchaseBase") + n(v, "euGoodsAcquisitionPurchaseBase"),
        )
        && eq(
            n(&v["rubrikker"], "rubrikB"),
            n(v, "foreignReverseChargeSalesBase"),
        )
        && eq(
            n(&v["rubrikker"], "rubrikC"),
            n(v, "exemptSalesBase") + n(v, "domesticReverseChargeSalesBase"),
        )
}
/// Business refusals preserve the original core result, including minimal nonregistration.
pub fn validate_original_vat_result(op: &str, v: &Value, success: bool) -> bool {
    if v["ok"] != success
        || !strings(&v["errors"])
        || success != v["errors"].as_array().unwrap().is_empty()
    {
        return false;
    }
    if !success {
        return true;
    }
    if !date(&v["periodStart"])
        || !date(&v["periodEnd"])
        || v["periodStart"].as_str() > v["periodEnd"].as_str()
        || !strings(&v["warnings"])
        || !strings(&v["appliedRules"])
    {
        return false;
    }
    match op {
        "report" => v["appliedRules"] == serde_json::json!(["DK-VAT-REPORT-001"]) && report(v),
        "eu-sales-list" => {
            let Some(rows) = v["customers"].as_array() else {
                return false;
            };
            let mut total = 0.;
            let mut count = 0u64;
            for r in rows {
                if !["vatNumber", "countryCode"]
                    .iter()
                    .all(|k| r[k].is_string())
                    || !(r["customerName"].is_null() || r["customerName"].is_string())
                    || !strings(&r["invoiceNumbers"])
                    || !r["totalValue"].is_number()
                {
                    return false;
                }
                let Some(c) = r["invoiceCount"].as_u64() else {
                    return false;
                };
                if c != r["invoiceNumbers"].as_array().unwrap().len() as u64 {
                    return false;
                }
                let Some(next) = count.checked_add(c) else {
                    return false;
                };
                count = next;
                total += n(r, "totalValue");
            }
            eq(total, n(v, "totalValue")) && v["invoiceCount"].as_u64() == Some(count)
        }
        "oss-report" | "filing" => {
            let r = &v["vatReport"];
            if !validate_original_vat_result("report", r, true)
                || r["periodStart"] != v["periodStart"]
                || r["periodEnd"] != v["periodEnd"]
            {
                return false;
            }
            if op == "oss-report" {
                v["appliedRules"] == serde_json::json!(["DK-VAT-OSS-001"])
                    && v["submission"] == false
                    && eq(n(v, "ossConsumerSalesBase"), n(r, "ossConsumerSalesBase"))
                    && v["entryCount"].is_u64()
                    && v["entryCount"] == r["ossConsumerSalesEntryCount"]
            } else {
                v["appliedRules"] == serde_json::json!(["DK-VAT-FILING-001"])
                    && matches!(v["periodStatus"].as_str(), Some("closed" | "reported"))
                    && v["vatPeriodType"] == r["vatPeriodType"]
                    && v["filingDeadline"] == r["filingDeadline"]
                    && (v["periodReference"].is_null() || v["periodReference"].is_string())
                    && v["rubrikker"] == r["rubrikker"]
            }
        }
        _ => false,
    }
}
impl ApiClient {
    pub async fn original_vat_report(&self, c: &str, from: &str, to: &str) -> Result<Value, Error> {
        self.original_vat_request(c, "report", from, to).await
    }
    pub async fn original_vat_eu_sales_list(
        &self,
        c: &str,
        from: &str,
        to: &str,
    ) -> Result<Value, Error> {
        self.original_vat_request(c, "eu-sales-list", from, to)
            .await
    }
    pub async fn original_vat_oss_report(
        &self,
        c: &str,
        from: &str,
        to: &str,
    ) -> Result<Value, Error> {
        self.original_vat_request(c, "oss-report", from, to).await
    }
    pub async fn original_vat_filing(&self, c: &str, from: &str, to: &str) -> Result<Value, Error> {
        self.original_vat_request(c, "filing", from, to).await
    }
    async fn original_vat_request(
        &self,
        c: &str,
        op: &str,
        from: &str,
        to: &str,
    ) -> Result<Value, Error> {
        if [from, to]
            .iter()
            .any(|s| s.len() > 4096 || s.chars().any(char::is_control))
        {
            return Err(Error::InvalidInput("bounded VAT selector required"));
        }
        let mut r = self
            .send(
                Method::GET,
                &company_path(c, &format!("original/vat/{op}"))?,
                &[("from", from), ("to", to)],
                None,
                self.cookie.as_ref().map(|s| s.as_str()),
            )
            .await
            .map_err(|e| {
                if e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = r.status().as_u16();
        if status == 408 || status >= 500 {
            return Err(Error::Transport);
        }
        if status != 200 && status != 422 {
            return Err(Error::Refused { status });
        }
        let mut bytes = vec![];
        while let Some(b) = r.chunk().await.map_err(|_| Error::Protocol)? {
            if bytes.len().saturating_add(b.len()) > MAX_RESPONSE {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&b);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        if !validate_original_vat_result(op, &v, status == 200)
            || status == 200 && (v["periodStart"] != from || v["periodEnd"] != to)
        {
            return Err(Error::Protocol);
        }
        Ok(v)
    }
}
