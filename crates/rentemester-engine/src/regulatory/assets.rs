//! Fixed checked-in source bytes. No runtime filesystem discovery.
pub const CATALOG: &str = include_str!("../../../../sources/legal-sources.json");
pub const INDEX: &str = include_str!("../../../../sources/downloaded/index.json");
pub const SCOPE: &str = include_str!("../../../../sources/scope.yaml");
pub const REFERENCES: &str = include_str!("legacy_references.json");
pub const XML: &[(&str, &str)] = &[
    (
        "DK-BILAG-OPBEVARING-2023-1383",
        include_str!("../../../../sources/downloaded/DK-BILAG-OPBEVARING-2023-1383.xml"),
    ),
    (
        "DK-BILAG-OPBEVARING-AMEND-2025-302",
        include_str!("../../../../sources/downloaded/DK-BILAG-OPBEVARING-AMEND-2025-302.xml"),
    ),
    (
        "DK-BOGFORINGSLOVEN-2022-700",
        include_str!("../../../../sources/downloaded/DK-BOGFORINGSLOVEN-2022-700.xml"),
    ),
    (
        "DK-DIGITAL-BOGFORING-NONREGISTERED-2024-205",
        include_str!(
            "../../../../sources/downloaded/DK-DIGITAL-BOGFORING-NONREGISTERED-2024-205.xml"
        ),
    ),
    (
        "DK-DIGITAL-STANDARD-BOGFORING-2023-97",
        include_str!("../../../../sources/downloaded/DK-DIGITAL-STANDARD-BOGFORING-2023-97.xml"),
    ),
    (
        "DK-MOMSBEKENDTGORELSEN-2023-1435",
        include_str!("../../../../sources/downloaded/DK-MOMSBEKENDTGORELSEN-2023-1435.xml"),
    ),
    (
        "DK-MOMSLOVEN-2024-209",
        include_str!("../../../../sources/downloaded/DK-MOMSLOVEN-2024-209.xml"),
    ),
    (
        "DK-RENTELOVEN-2014-459",
        include_str!("../../../../sources/downloaded/DK-RENTELOVEN-2014-459.xml"),
    ),
    (
        "DK-UDENRETLIGE-INDDRIVELSESOMKOSTNINGER-AMEND-2013-105",
        include_str!(
            "../../../../sources/downloaded/DK-UDENRETLIGE-INDDRIVELSESOMKOSTNINGER-AMEND-2013-105.xml"
        ),
    ),
];
pub const RULES: &[(&str, &str)] = &[
    (
        "annual-report",
        include_str!("../../../../rules/dk/annual-report.yaml"),
    ),
    ("bank", include_str!("../../../../rules/dk/bank.yaml")),
    (
        "bookkeeping",
        include_str!("../../../../rules/dk/bookkeeping.yaml"),
    ),
    (
        "documents",
        include_str!("../../../../rules/dk/documents.yaml"),
    ),
    ("email", include_str!("../../../../rules/dk/email.yaml")),
    ("gdpr", include_str!("../../../../rules/dk/gdpr.yaml")),
    (
        "invoices",
        include_str!("../../../../rules/dk/invoices.yaml"),
    ),
    (
        "mail-intake",
        include_str!("../../../../rules/dk/mail-intake.yaml"),
    ),
    ("mileage", include_str!("../../../../rules/dk/mileage.yaml")),
    ("peppol", include_str!("../../../../rules/dk/peppol.yaml")),
    (
        "tax-return",
        include_str!("../../../../rules/dk/tax-return.yaml"),
    ),
    ("vat", include_str!("../../../../rules/dk/vat.yaml")),
];
