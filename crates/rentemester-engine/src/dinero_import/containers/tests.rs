use super::*;
use std::io::Write;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
fn file(path: &str, bytes: &[u8]) -> SourceFile {
    SourceFile {
        path: path.into(),
        bytes: bytes.to_vec(),
    }
}
fn fixture() -> Vec<SourceFile> {
    vec![
        file("2025/Posteringer.csv", b"Konto;Kontonavn;Dato\n"),
        file(
            "Ikke-bogførte-bilag/kvittering-æøå.pdf",
            b"%PDF-1.4\nsynthetic original\n",
        ),
        file("2025/Kontoplan.csv", b"Nummer;Navn;Type\n"),
    ]
}
fn archive(files: &[SourceFile], method: CompressionMethod) -> Vec<u8> {
    let mut w = ZipWriter::new(Cursor::new(Vec::new()));
    for f in files {
        w.start_file(
            &f.path,
            SimpleFileOptions::default().compression_method(method),
        )
        .unwrap();
        w.write_all(&f.bytes).unwrap();
    }
    w.finish().unwrap().into_inner()
}
fn comparable(c: &Container) -> Vec<(&str, &[u8])> {
    c.files
        .iter()
        .map(|f| (f.path.as_str(), f.bytes.as_slice()))
        .collect()
}
#[test]
fn stored_deflated_and_json_preserve_all_unicode_bytes_and_inventory() {
    let files = fixture();
    let package = package(&files).unwrap();
    let json = decode(&package, Kind::Json).unwrap();
    assert_eq!(json.currency, "DKK");
    for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
        let raw = archive(&files, method);
        let zip = decode(&raw, Kind::Zip).unwrap();
        assert_eq!(zip.kind, Kind::Zip);
        assert_eq!(comparable(&zip), comparable(&json));
        assert_ne!(raw, package);
    }
    let mut reversed = fixture();
    reversed.reverse();
    assert_eq!(super::package(&reversed).unwrap(), package);
}
#[test]
fn json_preserves_currency_and_rejects_bad_contract_base64_or_aliases() {
    let bytes = package(&fixture()).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for n in 0..5 {
        let mut v = value.clone();
        match n {
            0 => v["format"] = "other".into(),
            1 => v["files"][0]["contentBase64"] = "Zg".into(),
            2 => v["files"][0]["path"] = "../bad.csv".into(),
            3 => {
                let duplicate = v["files"][0].clone();
                v["files"].as_array_mut().unwrap().push(duplicate);
            }
            _ => v["files"][0]["contentBase64"] = "Zh==".into(),
        };
        assert!(
            decode(&serde_json::to_vec(&v).unwrap(), Kind::Json).is_err(),
            "{n}"
        );
    }
    let mut v = value;
    v["currency"] = "EUR".into();
    assert_eq!(
        decode(&serde_json::to_vec(&v).unwrap(), Kind::Json)
            .unwrap()
            .currency,
        "EUR"
    );
}
#[test]
fn unsafe_paths_and_case_collisions_are_rejected_for_json_and_zip() {
    for name in [
        "../x.csv",
        "/x.csv",
        "C:/x.csv",
        "C:x.csv",
        "a/./x.csv",
        "a//x.csv",
        "a\\x.csv",
        "a/../x.csv",
        "a/hidden..csv",
        "a/x\n.csv",
    ] {
        let files = vec![file(name, b"x")];
        assert!(validate_paths(&files).is_err(), "{name}");
        assert!(
            decode(&archive(&files, CompressionMethod::Stored), Kind::Zip).is_err(),
            "{name}"
        );
    }
    let files = vec![file("Dir/A.csv", b"a"), file("dir/a.CSV", b"b")];
    assert!(package(&files).is_err());
    assert!(decode(&archive(&files, CompressionMethod::Stored), Kind::Zip).is_err());
}
#[test]
fn directory_markers_are_validated_and_never_strip_wrapper_roots() {
    let mut w = ZipWriter::new(Cursor::new(vec![]));
    w.add_directory("Wrapper/", SimpleFileOptions::default())
        .unwrap();
    w.add_directory("Wrapper/2025/", SimpleFileOptions::default())
        .unwrap();
    w.start_file("Wrapper/2025/Kontoplan.csv", SimpleFileOptions::default())
        .unwrap();
    w.write_all(b"chart").unwrap();
    let raw = w.finish().unwrap().into_inner();
    let c = decode(&raw, Kind::Zip).unwrap();
    assert_eq!(c.files.len(), 1);
    assert_eq!(c.files[0].path, "Wrapper/2025/Kontoplan.csv");
    let mut w = ZipWriter::new(Cursor::new(vec![]));
    w.add_directory("../escape/", SimpleFileOptions::default())
        .unwrap();
    w.start_file("a.csv", SimpleFileOptions::default()).unwrap();
    w.write_all(b"x").unwrap();
    assert!(decode(&w.finish().unwrap().into_inner(), Kind::Zip).is_err());
}
#[test]
fn symlinks_crc_corruption_and_truncation_refuse_complete_decode() {
    let mut w = ZipWriter::new(Cursor::new(vec![]));
    w.add_symlink("link.csv", "../../escape", SimpleFileOptions::default())
        .unwrap();
    assert!(decode(&w.finish().unwrap().into_inner(), Kind::Zip).is_err());
    let raw = archive(
        &[file("file.csv", b"unique-synthetic-payload")],
        CompressionMethod::Stored,
    );
    let mut corrupt = raw.clone();
    let pos = corrupt
        .windows(24)
        .position(|w| w == b"unique-synthetic-payload")
        .unwrap();
    corrupt[pos] ^= 1;
    assert!(decode(&corrupt, Kind::Zip).is_err());
    assert!(decode(&raw[..raw.len() - 12], Kind::Zip).is_err());
}
fn signatures(raw: &[u8], signature: &[u8]) -> Vec<usize> {
    raw.windows(signature.len())
        .enumerate()
        .filter_map(|(i, w)| (w == signature).then_some(i))
        .collect()
}
#[test]
fn duplicate_local_mismatch_encryption_and_unsupported_methods_refuse() {
    let raw = archive(
        &[file("a.csv", b"a"), file("b.csv", b"b")],
        CompressionMethod::Stored,
    );
    let mut duplicate = raw.clone();
    for at in signatures(&duplicate, b"b.csv") {
        duplicate[at] = b'a';
    }
    assert!(decode(&duplicate, Kind::Zip).is_err());
    let mut local = raw.clone();
    let at = signatures(&local, b"a.csv")[0];
    local[at] = b'z';
    assert!(decode(&local, Kind::Zip).is_err());
    for encrypt in [true, false] {
        let mut changed = raw.clone();
        for at in signatures(&changed, b"PK\x03\x04") {
            let offset = if encrypt { 6 } else { 8 };
            changed[at + offset] = if encrypt { 1 } else { 99 };
        }
        for at in signatures(&changed, b"PK\x01\x02") {
            let offset = if encrypt { 8 } else { 10 };
            changed[at + offset] = if encrypt { 1 } else { 99 };
        }
        assert!(decode(&changed, Kind::Zip).is_err());
    }
}
#[test]
fn decoded_size_and_file_count_limits_precede_unbounded_output() {
    let large = vec![0u8; MAX_FILE_BYTES + 1];
    assert!(validate_paths(&[file("large.bin", &large)]).is_err());
    assert!(
        decode(
            &archive(&[file("large.bin", &large)], CompressionMethod::Deflated),
            Kind::Zip
        )
        .is_err()
    );
    let ten = vec![0u8; 10 * 1024 * 1024];
    assert!(
        decode(
            &archive(
                &[file("one.bin", &ten), file("two.bin", &ten)],
                CompressionMethod::Deflated
            ),
            Kind::Zip
        )
        .is_err()
    );
    let mut files = (0..MAX_FILES)
        .map(|i| file(&format!("dir/{i}.csv"), b""))
        .collect::<Vec<_>>();
    let mut w = ZipWriter::new(Cursor::new(vec![]));
    w.add_directory("dir/", SimpleFileOptions::default())
        .unwrap();
    for f in &files {
        w.start_file(&f.path, SimpleFileOptions::default()).unwrap();
    }
    assert_eq!(
        decode(&w.finish().unwrap().into_inner(), Kind::Zip)
            .unwrap()
            .files
            .len(),
        MAX_FILES
    );
    files.push(file("extra.csv", b""));
    assert!(decode(&archive(&files, CompressionMethod::Stored), Kind::Zip).is_err());
    assert!(decode(&vec![0u8; MAX_CONTAINER_BYTES + 1], Kind::Zip).is_err());
}
