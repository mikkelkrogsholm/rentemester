//! Bounded immutable upload bytes. Paths are generated from content, never supplied by HTTP.
use sha2::{Digest, Sha256};
pub const MAX_BYTES: usize = 25 * 1024 * 1024;
pub struct Prepared {
    bytes: Vec<u8>,
    filename: String,
    sha256: String,
    extension: String,
    mime: &'static str,
}
impl Prepared {
    /// Import-only raw source. Ordinary document ingestion does not admit ZIP.
    pub fn dinero_source(filename: String, bytes: Vec<u8>) -> Result<Self, &'static str> {
        if !filename.to_ascii_lowercase().ends_with(".zip") {
            return Self::new(filename, bytes);
        }
        if bytes.is_empty()
            || bytes.len() > MAX_BYTES
            || filename.len() > 255
            || filename.contains(['/', '\\'])
            || filename.chars().any(char::is_control)
            || !(bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06"))
        {
            return Err("invalid Dinero ZIP source");
        }
        Ok(Self {
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            bytes,
            filename,
            extension: "zip".into(),
            mime: "application/zip",
        })
    }
    pub fn new(filename: String, bytes: Vec<u8>) -> Result<Self, &'static str> {
        if bytes.is_empty()
            || bytes.len() > MAX_BYTES
            || filename.is_empty()
            || filename.len() > 255
            || filename.contains(['/', '\\'])
            || filename.chars().any(char::is_control)
        {
            return Err("invalid document file");
        }
        let extension = filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let mime = match extension.as_str() {
            "pdf" => "application/pdf",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "txt" => "text/plain",
            "json" => "application/json",
            "xml" => "application/xml",
            _ => return Err("unsupported document extension"),
        };
        let sniffed = if bytes.starts_with(b"%PDF-") {
            Some("application/pdf")
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some("image/png")
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            Some("image/jpeg")
        } else {
            None
        };
        if (matches!(extension.as_str(), "pdf" | "png" | "jpg" | "jpeg") && sniffed != Some(mime))
            || sniffed.is_some_and(|m| m != mime)
        {
            return Err("document content does not match extension");
        }
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        Ok(Self {
            bytes,
            filename,
            sha256,
            extension,
            mime,
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn filename(&self) -> &str {
        &self.filename
    }
    pub fn hash(&self) -> &str {
        &self.sha256
    }
    pub fn mime(&self) -> &'static str {
        self.mime
    }
    pub fn relative_path(&self) -> String {
        format!("documents/originals/{}.{}", self.sha256, self.extension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zip_is_an_import_only_original_with_its_own_content_address() {
        let bytes =
            b"PK\x03\x04Synthetic ZIP header; full inventory is checked by the import adapter"
                .to_vec();
        assert!(Prepared::new("export.zip".into(), bytes.clone()).is_err());
        let prepared = Prepared::dinero_source("export.ZIP".into(), bytes.clone()).unwrap();
        assert_eq!(prepared.mime(), "application/zip");
        assert_eq!(prepared.bytes(), bytes);
        assert!(prepared.relative_path().ends_with(".zip"));
        for name in ["../export.zip", "dir/export.zip", "export\n.zip"] {
            assert!(Prepared::dinero_source(name.into(), bytes.clone()).is_err());
        }
        assert!(Prepared::dinero_source("export.zip".into(), b"not a ZIP".to_vec()).is_err());
    }
    #[test]
    fn original_identity_is_content_bound_and_refuses_paths_or_spoofed_type() {
        let bytes = b"Synthetic receipt\n".to_vec();
        let first = Prepared::new("synthetic.txt".into(), bytes.clone()).unwrap();
        let renamed = Prepared::new("renamed.txt".into(), bytes.clone()).unwrap();
        assert_eq!(first.relative_path(), renamed.relative_path());
        assert_ne!(
            first.hash(),
            Prepared::new("synthetic.txt".into(), b"Different receipt".to_vec())
                .unwrap()
                .hash()
        );
        for filename in [
            "../receipt.txt",
            "sub/receipt.txt",
            "sub\\receipt.txt",
            "receipt\n.txt",
            "receipt.pdf",
            "receipt.exe",
        ] {
            assert!(
                Prepared::new(filename.into(), bytes.clone()).is_err(),
                "accepted {filename:?}"
            );
        }
        assert!(Prepared::new("receipt.txt".into(), b"%PDF-1.4\n".to_vec()).is_err());
        assert!(Prepared::new("empty.txt".into(), vec![]).is_err());
    }
}
