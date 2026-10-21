use super::*;
fn d(s: &str) -> Date {
    Date::parse(s).unwrap()
}
fn m(s: &str) -> Money {
    Money::parse(s).unwrap()
}
fn history() -> History {
    History {
        currency: "DKK".into(),
        gross: m("1000"),
        effective_due_date: d("2025-01-01"),
        events: vec![],
        claims: vec![],
        corrections: vec![],
    }
}
fn calc(h: &History, on: &str) -> Calculation {
    calculate(
        h,
        &Calculate {
            as_of: d(on),
            reference_basis_points: Some(200),
        },
    )
    .unwrap()
}
fn claim(id: i64, on: &str, amount: &str, posted: bool) -> InterestClaim {
    InterestClaim {
        id,
        claim_date: d(on),
        reference_basis_points: 200,
        annual_basis_points: 1000,
        rate_source: RateSource::ManualOverride,
        amount: m(amount),
        posted_journal_id: posted.then_some(100 + id),
    }
}
fn event(kind: EventKind, id: i64, on: &str, amount: &str) -> PrincipalEvent {
    PrincipalEvent {
        kind,
        source_id: id,
        effective_date: d(on),
        amount: m(amount),
    }
}
#[test]
fn exact_dated_principal_and_staged_claims() {
    let mut h = history();
    assert_eq!(calc(&h, "2025-01-11").accrued_interest_amount, m("2.74"));
    assert_eq!(calc(&h, "2025-01-21").accrued_interest_amount, m("5.48"));
    h.claims.push(claim(1, "2025-01-11", "2.74", true));
    assert_eq!(calc(&h, "2025-01-21").accrued_interest_amount, m("2.74"));
    h.events
        .push(event(EventKind::Payment, 1, "2025-01-11", "500"));
    let c = calc(&h, "2025-01-21");
    assert_eq!(c.total_interest_to_date, m("4.11"));
    assert_eq!(c.accrued_interest_amount, m("1.37"));
    h.claims.clear();
    h.events = vec![
        event(EventKind::Payment, 1, "2025-01-06", "500"),
        event(EventKind::Refund, 1, "2025-01-11", "200"),
    ];
    assert_eq!(calc(&h, "2025-01-21").total_interest_to_date, m("3.97"));
}
#[test]
fn causal_correction_is_separate_from_account_capacity() {
    let mut h = history();
    h.claims.push(claim(1, "2025-01-11", "2.74", true));
    assert_eq!(correction(&h).unwrap().over_claimed_amount, m("0"));
    h.events
        .push(event(EventKind::Payment, 1, "2025-01-06", "500"));
    let c = correction(&h).unwrap();
    assert_eq!(c.lawful_interest, m("2.05"));
    assert_eq!(c.over_claimed_amount, m("0.69"));
    assert_eq!(c.claim_overclaims[0].claim_id, 1);
    h.corrections.push(InterestCorrection {
        id: 1,
        correction_date: d("2025-01-12"),
        amount: m("0.69"),
        journal_id: 200,
    });
    assert_eq!(correction(&h).unwrap().over_claimed_amount, m("0"));
    assert_eq!(calc(&h, "2025-01-21").accrued_interest_amount, m("2.06"));
}
#[test]
fn table_halfyears_and_old_claims_share_the_same_windows() {
    let mut h = history();
    h.effective_due_date = d("2025-06-21");
    let input = Calculate {
        as_of: d("2025-07-11"),
        reference_basis_points: None,
    };
    let c = calculate(&h, &input).unwrap();
    assert_eq!(c.total_interest_to_date, m("5.62"));
    h.claims.push(InterestClaim {
        id: 1,
        claim_date: input.as_of,
        reference_basis_points: 175,
        annual_basis_points: 975,
        rate_source: RateSource::StatutoryTable,
        amount: c.accrued_interest_amount,
        posted_journal_id: Some(1),
    });
    assert_eq!(correction(&h).unwrap().over_claimed_amount, m("0"));
    assert_eq!(
        calculate(&h, &input).unwrap().total_interest_to_date,
        m("5.62")
    );
    let c = calculate(
        &h,
        &Calculate {
            as_of: d("2025-07-21"),
            reference_basis_points: None,
        },
    )
    .unwrap();
    assert_eq!(c.total_interest_to_date, m("8.29"));
    assert_eq!(c.accrued_interest_amount, m("2.67"));
}
#[test]
fn staged_rounding_does_not_sum_independently_rounded_segments() {
    let mut h = history();
    h.gross = m("1");
    // 10% of 1 kr for two ten-day windows is 0.005479... kr -> one øre,
    // while rounding each individual window first would incorrectly give zero.
    assert_eq!(calc(&h, "2025-01-21").total_interest_to_date, m("0.01"));
    h.events
        .push(event(EventKind::Payment, 1, "2025-01-11", "0.01"));
    assert_eq!(calc(&h, "2025-01-21").total_interest_to_date, m("0.01"));
    // More realistic successive positive registrations also preserve cumulative øre.
    h = history();
    h.gross = m("10");
    let first = calc(&h, "2025-01-11").accrued_interest_amount;
    assert_eq!(first, m("0.03"));
    h.claims.push(claim(1, "2025-01-11", "0.03", false));
    let second = calc(&h, "2025-01-21");
    assert_eq!(second.total_interest_to_date, m("0.05"));
    assert_eq!(second.accrued_interest_amount, m("0.02"));
}
#[test]
fn unposted_claims_anchor_but_do_not_enter_posted_correction_sum() {
    let mut h = history();
    h.claims = vec![
        claim(2, "2025-01-21", "2.74", true),
        claim(1, "2025-01-11", "2.74", false),
    ];
    let c = correction(&h).unwrap();
    assert_eq!(c.posted_interest, m("2.74"));
    assert_eq!(c.lawful_interest, m("2.74"));
    assert!(!c.has_proposal);
    h.events
        .push(event(EventKind::Credit, 1, "2025-01-11", "500"));
    let c = correction(&h).unwrap();
    assert_eq!(c.lawful_interest, m("1.37"));
    assert_eq!(c.over_claimed_amount, m("1.37"));
    assert_eq!(c.claim_overclaims.len(), 1);
    assert_eq!(c.claim_overclaims[0].claim_id, 2);
}
#[test]
fn credits_writeoffs_and_same_day_effects_and_query_horizon() {
    let mut h = history();
    h.events
        .push(event(EventKind::WriteOff, 1, "2025-01-01", "500"));
    assert_eq!(calc(&h, "2025-01-11").total_interest_to_date, m("1.37"));
    h.events
        .push(event(EventKind::Credit, 1, "2025-01-12", "500"));
    assert_eq!(calc(&h, "2025-01-11").principal_open_balance, m("500"));
    assert_eq!(calc(&h, "2025-01-21").claimable_days, 0);
    h.events
        .push(event(EventKind::Refund, 1, "2025-01-22", "100"));
    assert_eq!(calc(&h, "2025-01-23").principal_open_balance, m("100"));
    assert_eq!(
        calc(&history(), "2025-01-01").accrued_interest_amount,
        m("0")
    );
}
#[test]
fn invalid_history_foreign_unknown_table_and_manual_warning() {
    let mut h = history();
    h.currency = "EUR".into();
    assert!(
        calculate(
            &h,
            &Calculate {
                as_of: d("2025-01-11"),
                reference_basis_points: Some(200)
            }
        )
        .is_err()
    );
    h = history();
    h.events = vec![
        event(EventKind::Payment, 1, "2025-01-03", "5"),
        event(EventKind::Payment, 1, "2025-01-04", "5"),
    ];
    assert!(correction(&h).is_err());
    h = history();
    h.claims.push(claim(1, "2025-01-11", "0", true));
    assert!(correction(&h).is_err());
    h = history();
    h.claims.push(claim(1, "2025-01-11", "2.74", true));
    h.corrections.push(InterestCorrection {
        id: 1,
        correction_date: d("2025-01-12"),
        amount: m("1"),
        journal_id: 101,
    });
    assert!(correction(&h).is_err());
    h = history();
    h.effective_due_date = d("2022-01-01");
    assert!(
        calculate(
            &h,
            &Calculate {
                as_of: d("2022-02-01"),
                reference_basis_points: None
            }
        )
        .is_err()
    );
    assert!(
        calculate(
            &h,
            &Calculate {
                as_of: d("2022-02-01"),
                reference_basis_points: Some(200)
            }
        )
        .is_ok()
    );
    assert_eq!(calc(&history(), "2025-01-11").warnings.len(), 1);
    assert!(
        calculate(
            &history(),
            &Calculate {
                as_of: d("2025-01-11"),
                reference_basis_points: Some(u32::MAX)
            }
        )
        .is_err()
    );
}

#[test]
fn later_effective_refund_does_not_invalidate_a_previous_lawful_correction() {
    let mut h = history();
    h.claims.push(claim(1, "2025-01-11", "2.74", true));
    h.events
        .push(event(EventKind::Payment, 1, "2025-01-06", "500"));
    assert_eq!(correction(&h).unwrap().over_claimed_amount, m("0.69"));
    h.corrections.push(InterestCorrection {
        id: 1,
        correction_date: d("2025-01-12"),
        amount: m("0.69"),
        journal_id: 200,
    });
    // A subsequently discovered refund is effective within the original window.
    // Its immutable admission/proof is storage's responsibility.
    h.events
        .push(event(EventKind::Refund, 1, "2025-01-08", "500"));
    let proposal = correction(&h).unwrap();
    assert_eq!(proposal.lawful_interest, m("2.47"));
    assert_eq!(proposal.already_corrected, m("0.69"));
    assert_eq!(proposal.over_claimed_amount, m("0"));
    assert!(!proposal.has_proposal);
    assert_eq!(proposal.claim_overclaims[0].over_claimed_amount, m("0.27"));
}
#[test]
fn historical_query_uses_later_claims_rate_but_not_its_billed_amount() {
    let mut h = history();
    let mut later = claim(1, "2025-01-21", "10.96", true);
    later.reference_basis_points = 1200;
    later.annual_basis_points = 2000;
    h.claims.push(later);
    // TS includes all claims, clamps their windows to asOf, and only counts
    // actually billed claims dated <= asOf. It does not filter future anchors.
    let c = calc(&h, "2025-01-11");
    assert_eq!(c.total_interest_to_date, m("5.48"));
    assert_eq!(c.prior_claimed_interest, m("0"));
    assert_eq!(c.claimable_days, 0);
    assert_eq!(c.interest_from_date, d("2025-01-11"));
    assert_eq!(c.accrued_interest_amount, m("0"));
}
