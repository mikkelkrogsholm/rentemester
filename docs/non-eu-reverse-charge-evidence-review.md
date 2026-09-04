# Non-EU reverse-charge material review

`documents review-non-eu-reverse-charge-evidence` is a narrow exception path for a formally deficient invoice for a service from a supplier established outside the EU. It is not a VAT override.

The original document and metadata remain immutable. The review is append-only and binds the document and payload hashes to separately hashed supplier, buyer and service evidence, a Danish tax period, an explicit deduction percentage and an authenticated service principal. Exact retry is idempotent; changed evidence requires the current review hash as an explicit supersession reference.

The ordinary automatic invoice gate remains in force until a valid review exists. The review rejects a contradictory buyer, unresolved/non-matching supplier establishment, foreign or local supplier VAT, missing material sources, stale hashes and posted or linked documents. An observed OSS or IE VAT identifier on a supplier documented as established in the US is not treated as EU establishment or a documented home-country registration number.

For a partial deduction, Rentemester records the full Danish reverse-charge output VAT, deducts only the documented percentage as input VAT, and includes the remaining VAT in the expense. It never uses `exempt` as a technical fallback.
