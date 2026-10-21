use rentemester_client::write_saft_package;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
fn tar(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut out = vec![];
    for (name, data) in files {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
        h[148..156].fill(b' ');
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        let sum: usize = h.iter().map(|b| *b as usize).sum();
        h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        out.extend(h);
        out.extend(data);
        out.resize(out.len().next_multiple_of(512), 0);
    }
    out.resize(out.len() + 1024, 0);
    out
}

const ROOT: &str = "saft-export-synthetic";
fn fixture() -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::from([
        (
            "README.txt".to_owned(),
            b"Synthetic bounded profile".to_vec(),
        ),
        (
            "saft.xml".to_owned(),
            b"<AuditFile>synthetic</AuditFile>".to_vec(),
        ),
    ]);
    let outputs:Vec<_>=files.iter().map(|(path,b)|json!({"path":path,"sha256":format!("{:x}",Sha256::digest(b)),"sizeBytes":b.len()})).collect();
    let manifest = json!({"manifestVersion":2,"packageType":"saft_export","profileId":"rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles","generatedAt":"2026-12-31T23:59:59.000Z","generatedAtExplicit":false,"generatedAtSource":"period_end","periodStart":"2026-01-01","periodEnd":"2026-12-31","sourceCompanyRootName":"synthetic","appliedRules":["DK-BOOKKEEPING-SAFT-EXPORT-001"],"files":{"saftXml":"saft.xml","readme":"README.txt"},"counts":{"accounts":0,"journalEntries":0,"salesInvoices":0,"purchaseInvoices":0,"vatSummaryCodes":0,"documentReferences":0,"customers":0,"suppliers":0},"outputs":outputs,"sourceContext":{"journalEntries":[],"documents":[],"auditLog":[],"trialBalance":{"ok":true,"balanced":true,"accounts":[],"totalDebit":"0.00","totalCredit":"0.00"},"journalHashChain":{"ok":true,"entriesChecked":0,"errors":[]}}});
    files.insert(
        "manifest.json".to_owned(),
        serde_json::to_vec(&manifest).unwrap(),
    );
    files
        .into_iter()
        .map(|(p, b)| (format!("{ROOT}/{p}"), b))
        .collect()
}
fn envelope(files: BTreeMap<String, Vec<u8>>) -> Value {
    json!({"ok":true,"export":{"contentType":"application/x-tar","bytes":tar(&files)}})
}
#[test]
fn exact_flat_metadata_and_fresh_private_three_file_package() {
    let out = std::env::temp_dir().join(format!("rm137-valid-{}", std::process::id()));
    std::fs::create_dir(&out).unwrap();
    std::fs::write(out.join("existing"), b"keep").unwrap();
    let files = fixture();
    let value = envelope(files.clone());
    let result = write_saft_package(&value, &out).unwrap();
    let target = std::fs::canonicalize(&out).unwrap().join(ROOT);
    assert_eq!(
        result,
        json!({"ok":true,"exportDir":target,"manifestPath":target.join("manifest.json"),"saftXmlPath":target.join("saft.xml"),"generatedAt":"2026-12-31T23:59:59.000Z","periodStart":"2026-01-01","periodEnd":"2026-12-31","journalEntryCount":0,"salesInvoiceCount":0,"purchaseInvoiceCount":0,"customerCount":0,"supplierCount":0,"appliedRules":["DK-BOOKKEEPING-SAFT-EXPORT-001"],"errors":[]})
    );
    for (name, bytes) in files {
        assert_eq!(std::fs::read(out.join(name)).unwrap(), bytes);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(target.join("saft.xml"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert!(write_saft_package(&value, &out).is_err());
    assert_eq!(std::fs::read(out.join("existing")).unwrap(), b"keep");
    std::fs::remove_dir_all(out).unwrap();
}
#[test]
fn finite_manifest_source_hash_and_count_refusals_precede_writes() {
    for kind in 0..9 {
        let mut files = fixture();
        let key = format!("{ROOT}/manifest.json");
        let mut manifest: Value = serde_json::from_slice(&files[&key]).unwrap();
        match kind {
            0 => manifest["outputs"][0]["sha256"] = json!("forged"),
            1 => manifest["counts"]["customers"] = Value::Null,
            2 => manifest["sourceContext"]["documents"] = Value::Null,
            3 => manifest["sourceContext"]["journalHashChain"]["ok"] = json!(false),
            4 => manifest["profileId"] = json!("unsupported"),
            5 => manifest["files"]["saftXml"] = json!("../escape"),
            6 => manifest["outputs"][1] = manifest["outputs"][0].clone(),
            7 => {
                files.insert(format!("{ROOT}/extra"), vec![]);
            }
            _ => manifest["sourceContext"]["trialBalance"]["totalDebit"] = Value::Null,
        };
        files.insert(key, serde_json::to_vec(&manifest).unwrap());
        let out = std::env::temp_dir().join(format!("rm137-invalid-{}-{kind}", std::process::id()));
        assert!(
            write_saft_package(&envelope(files), &out).is_err(),
            "case {kind}"
        );
        assert!(!out.exists());
    }
}
