use super::*;

#[test]
fn dates_require_exact_unique_valid_ordered_selectors() {
    for value in [
        "from=2026-01-01&to=2026-03-31",
        "to=2026-03-31&from=2026-01-01",
        "from=2024-02-29&to=2024-02-29",
    ] {
        assert!(dates(value).is_ok(), "{value}");
    }
    for value in [
        "",
        "from=2026-01-01",
        "to=2026-01-01",
        "from=2026-03-31&to=2026-01-01",
        "from=2026-02-30&to=2026-03-31",
        "from=2026-01-01&from=2026-01-01&to=2026-03-31",
        "from=2026-01-01&to=2026-03-31&to=2026-03-31",
        "from=2026-01-01&to=2026-03-31&confirm=true",
        "from=2026-01-01&to=2026-03-31&company=b",
        "from=2026-01-01&to=2026-03-31&year=2026",
        "from=2026-01-01&to=2026-03-31&source=archive",
        "from=%XX&to=2026-03-31",
        "from=%&to=2026-03-31",
        "from=2026-1-1&to=2026-03-31",
        "from=2026-01-01&to=2026-03-31%00",
    ] {
        assert!(dates(value).is_err(), "{value}");
    }
    assert!(dates(&"x".repeat(129)).is_err());
}
