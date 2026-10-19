use super::*;
use serde_json::json;
fn f(owner: &str, child: &str, bp: u16, from: &str) -> Fact {
    serde_json::from_value::<Fact>(json!({"owner":{"kind":"company","companySlug":owner},"ownedCompanySlug":child,"validFrom":from,"economicBasisPoints":bp,"controlType":"equity","jurisdiction":"dk","evidenceRefs":["synthetic"]})).unwrap().normalize().unwrap()
}
#[test]
fn normalized_dates_values_and_intervals_are_explicit() {
    let v = f("a", "b", 4000, "2026-01-01");
    assert_eq!(v.jurisdiction, "DK");
    assert!(f("a", "b", 4000, "2026-01-01").active("2026-01-01"));
    for bad in ["2026-02-30", "2026-13-01", "2026-1-01"] {
        assert!(date(bad).is_err());
    }
    let mut v = v;
    v.economic_interval_basis_points = Some(Interval { min: 0, max: 5000 });
    assert!(v.clone().normalize().is_err());
    v.economic_basis_points = None;
    assert!(v.clone().normalize().is_ok());
    v.economic_interval_basis_points = Some(Interval {
        min: 7000,
        max: 6000,
    });
    assert!(v.normalize().is_err());
}
#[test]
fn effective_slices_reject_duplicates_overlaps_excess_floors_and_cycles() {
    let a = f("a", "b", 7000, "2026-01-01");
    let b = f("c", "b", 4000, "2026-01-01");
    assert!(integrity(&[], &[("a".into(), a.clone()), ("b".into(), b)]).is_err());
    assert!(integrity(&[], &[("a".into(), a.clone()), ("a".into(), a.clone())]).is_err());
    assert!(
        integrity(
            &[],
            &[
                ("a".into(), a.clone()),
                ("b".into(), f("a", "b", 6000, "2026-01-01"))
            ]
        )
        .is_err()
    );
    assert!(
        integrity(
            &[],
            &[
                ("a".into(), a.clone()),
                ("b".into(), f("b", "a", 1000, "2026-01-01"))
            ]
        )
        .is_err()
    );
    let mut interval = a;
    interval.economic_basis_points = None;
    interval.economic_interval_basis_points = Some(Interval {
        min: 7000,
        max: 10000,
    });
    let mut other = interval.clone();
    other.owner = Endpoint::Company {
        company_slug: "c".into(),
    };
    assert!(integrity(&[], &[("a".into(), interval), ("b".into(), other)]).is_err());
}
#[test]
fn later_fact_has_hash_bound_ending_and_exclusive_boundary() {
    let old = f("a", "b", 6000, "2026-01-01");
    let next = f("a", "b", 7000, "2026-06-01");
    let end = integrity(&[("old".into(), old.clone())], &[("next".into(), next)]).unwrap();
    assert_eq!(
        end,
        [Ending {
            fact_hash: "old".into(),
            successor_fact_hash: "next".into(),
            effective_to_exclusive: "2026-06-01".into()
        }]
    );
    let old = old.with_end(Some(&end[0].effective_to_exclusive));
    assert!(old.active("2026-05-31"));
    assert!(!old.active("2026-06-01"));
    assert!(integrity(&[("old".into(), old)], &[]).unwrap().is_empty());
}
