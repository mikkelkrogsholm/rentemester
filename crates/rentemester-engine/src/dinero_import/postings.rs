use super::*;
type ParsedPostings = (Vec<Line>, Option<Date>, Vec<Voucher>);
pub(super) fn parse(
    file: &SourceFile,
    accounts: &BTreeSet<String>,
) -> Result<ParsedPostings, Error> {
    let rows = csv::read(
        &file.bytes,
        &[
            "Konto",
            "Kontonavn",
            "Dato",
            "Bilag",
            "Bilagstype",
            "Tekst",
            "Momstype",
            "Beløb",
            "Saldo",
        ],
    )?;
    let mut opening = vec![];
    let mut dates = BTreeSet::new();
    let mut vouchers: Vec<Voucher> = vec![];
    for r in rows {
        if !accounts.contains(&r[0]) {
            return Err(refused(format!(
                "posting references unknown account {}",
                r[0]
            )));
        }
        let date = Date::parse(&r[2]).map_err(|_| refused("posting date is not ISO"))?;
        let m = decimal(&r[7])?;
        let zero = m == Money::default();
        let primo = r[3] == "0" && r[5].to_lowercase() == "primobeholdning";
        if primo {
            dates.insert(date);
        } else if r[3] == "0" || r[5].to_lowercase() == "primobeholdning" {
            continue;
        }
        if zero {
            continue;
        }
        let debit = if m.is_negative() {
            Money::default()
        } else {
            m.clone()
        };
        let credit = if m.is_negative() {
            Money::default().subtract(&m)
        } else {
            Money::default()
        };
        let mut line = Line {
            account_no: r[0].clone(),
            debit_amount: debit,
            credit_amount: credit,
            vat_code: None,
            text: Some(r[5].clone()),
        };
        if primo {
            opening.push(line);
            continue;
        }
        if r[3].is_empty() {
            return Err(refused("posting needs voucher reference"));
        }
        if !r[6].is_empty() {
            line.vat_code = Some(
                vat(&r[6])
                    .ok_or_else(|| refused(format!("unsupported voucher Momstype {}", r[6])))?
                    .into(),
            );
        }
        if let Some(v) = vouchers.iter_mut().find(|v| v.voucher_ref == r[3]) {
            if v.transaction_date != date {
                return Err(refused("voucher contains different dates"));
            }
            if r[5].is_empty() {
                line.text = Some(v.text.clone());
            }
            v.lines.push(line);
        } else {
            let text = if r[5].is_empty() {
                format!("Bilag {}", r[3])
            } else {
                r[5].clone()
            };
            line.text = Some(text.clone());
            vouchers.push(Voucher {
                voucher_ref: r[3].clone(),
                transaction_date: date,
                text,
                entry_type: r[4].clone(),
                lines: vec![line],
            });
        }
    }
    if dates.len() > 1 {
        return Err(refused("opening rows have multiple dates"));
    }
    let cut = if opening.is_empty() {
        None
    } else {
        dates.first().copied()
    };
    if !opening.is_empty() {
        balanced(&opening, "opening")?;
    }
    for v in &vouchers {
        balanced(&v.lines, &v.voucher_ref)?;
        if cut.is_some_and(|d| v.transaction_date < d) {
            return Err(refused("historical voucher precedes opening"));
        }
    }
    Ok((opening, cut, vouchers))
}
fn balanced(lines: &[Line], label: &str) -> Result<(), Error> {
    let mut debit = Money::default();
    let mut credit = Money::default();
    for l in lines {
        debit.add(&l.debit_amount);
        credit.add(&l.credit_amount);
    }
    if lines.len() < 2 || debit != credit {
        return Err(refused(format!(
            "voucher {label} must have at least two balanced lines"
        )));
    }
    Ok(())
}
/// The source-defined 64040/64060 pair admits one inferred EU service base only
/// when its exact 25% controls reconcile within the source's one-øre tolerance.
pub(super) fn classify_reverse_charge(
    vouchers: &mut [Voucher],
    chart: &[Account],
) -> Result<(), Error> {
    let expense = |no: &str| {
        chart
            .iter()
            .any(|a| a.account_no == no && a.kind == "expense")
    };
    for v in vouchers {
        let output = v
            .lines
            .iter()
            .filter(|l| l.account_no == "64040")
            .collect::<Vec<_>>();
        let input = v
            .lines
            .iter()
            .filter(|l| l.account_no == "64060")
            .collect::<Vec<_>>();
        if output.is_empty() || input.is_empty() {
            continue;
        }
        let mut out = Money::default();
        for l in output {
            out.add(&l.credit_amount);
            out = out.subtract(&l.debit_amount);
        }
        let mut inp = Money::default();
        for l in input {
            inp.add(&l.debit_amount);
            inp = inp.subtract(&l.credit_amount);
        }
        if out == Money::default() || out.is_negative() || out != inp {
            return Err(refused("inconsistent reverse-charge control pair"));
        }
        let explicit = v
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                expense(&l.account_no)
                    && matches!(
                        l.vat_code.as_deref(),
                        Some("EU_SERVICE_REVERSE_CHARGE" | "NON_EU_SERVICE_REVERSE_CHARGE")
                    )
            })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let mut codes = BTreeSet::new();
        for i in &explicit {
            codes.insert(v.lines[*i].vat_code.clone().unwrap());
        }
        if codes.len() > 1 {
            return Err(refused("mixed EU and non-EU reverse-charge bases"));
        }
        let near = |a: &Money, b: &Money| {
            a.subtract(b).0.magnitude() <= &num_bigint::BigInt::from(1u8).magnitude().clone()
        };
        let indexes = if explicit.is_empty() {
            if v.lines
                .iter()
                .any(|l| expense(&l.account_no) && l.vat_code.is_some())
            {
                return Err(refused("conflicting reverse-charge base VAT code"));
            }
            let candidates = v
                .lines
                .iter()
                .enumerate()
                .filter(|(_, l)| {
                    let net = l.debit_amount.subtract(&l.credit_amount);
                    expense(&l.account_no)
                        && !net.is_negative()
                        && net != Money::default()
                        && near(&net.percent("25").expect("fixed percent"), &out)
                })
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if candidates.len() != 1 {
                return Err(refused(
                    "reverse-charge controls require one matching expense base",
                ));
            }
            candidates
        } else {
            explicit
        };
        let mut base = Money::default();
        for i in &indexes {
            base.add(&v.lines[*i].debit_amount);
            base = base.subtract(&v.lines[*i].credit_amount);
        }
        if !near(&base.percent("25")?, &out) {
            return Err(refused("reverse-charge base does not reconcile"));
        }
        let code = codes
            .first()
            .cloned()
            .unwrap_or_else(|| "EU_SERVICE_REVERSE_CHARGE".into());
        for i in indexes {
            v.lines[i].vat_code = Some(code.clone());
        }
    }
    Ok(())
}
