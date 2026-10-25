//! Honest HMAC company-artifact qualification, independent of native signing receipts.
use super::*;
pub(crate) struct Retained {
    pub id: String,
    pub at: String,
    pub ms: i64,
    pub ids: [i64; 3],
    pub hash: String,
    pub size: u64,
    pub bytes: Vec<u8>,
}
pub(crate) fn retained(
    c: &Transaction<'_>,
    l: &Transaction<'_>,
    slug: &str,
) -> Result<Vec<Retained>, Error> {
    let filename = |db: &Transaction<'_>| {
        db.query_row(
            "SELECT file FROM pragma_database_list WHERE name='main'",
            [],
            |r| r.get::<_, String>(0),
        )
        .map_err(|_| invalid())
    };
    let cp = filename(c)?;
    let lp = filename(l)?;
    // In-memory native fixtures have no physical retained company resources.
    if cp.is_empty() && lp.is_empty() {
        return Ok(vec![]);
    }
    let uuid: String = l
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(|_| invalid())?;
    let proofs = rentemester_runtime::namespace::observe_original717(
        std::path::Path::new(&cp),
        std::path::Path::new(&lp),
        slug,
    )
    .map_err(|_| invalid())?;
    let mut out = vec![];
    for proof in proofs {
        let m = proof.manifest();
        let ids = rentemester_storage::backup_state::original717::validate_image(
            &proof.files()["ledger.sqlite"],
            &uuid,
            proof.files(),
        )?;
        let ms = rentemester_storage::backup_state::original122_moment(&m.created_at)?;
        let bytes = rentemester_runtime::backup_artifact::original717::archive(proof.files())
            .map_err(|_| invalid())?;
        out.push(Retained {
            id: m.backup_id.clone(),
            at: m.created_at.clone(),
            ms,
            ids,
            hash: rentemester_runtime::backup_artifact::original717::sha256(&bytes),
            size: bytes.len() as u64,
            bytes,
        });
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.ms));
    Ok(out)
}
