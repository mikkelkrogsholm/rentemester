//! Source CLI custody names. Product routes own credentials, paths never supply authority.
pub fn endpoint(command: &str) -> Option<&'static str> {
    match command {
        "backup" => Some("original-system-backup-signed"),
        "export-public-key" => Some("original-system-export-public-key"),
        "rotate-backup-keypair" => Some("original-system-rotate-backup-keypair"),
        "verify-backup-signature" => Some("original-system-verify-backup-signature"),
        _ => None,
    }
}
