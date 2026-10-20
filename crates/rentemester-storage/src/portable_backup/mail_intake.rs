//! Fixed mail evidence validation; IMAP credentials and polling consent are never archived.
use rentemester_engine::Error;
use rusqlite::Connection;
/// Company-local receipt/audit/document proof. Physical originals are verified by Owner.
pub fn validate_mail_intake(tx: &Connection) -> Result<(), Error> {
    crate::mail_intake::validate_mail_intake(tx)
}
/// Cross-company namespace inventory proof, including completed and split-recovery documents.
pub fn validate_company(
    control: &Connection,
    company: &Connection,
    slug: &str,
) -> Result<(), Error> {
    crate::mail_intake::cross_validate(control, company, slug)
}
pub(super) fn validate_control(control: &Connection) -> Result<(), Error> {
    crate::mail_intake::validate_control(control)
}
