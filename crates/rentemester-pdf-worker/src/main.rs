//! One original PDF per process. No workspace, database, network, or system PDFium lookup.
use pdfium_render::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    time::Duration,
};
use unicode_normalization::UnicodeNormalization;

const MAX_INPUT: usize = 10 * 1024 * 1024;
const MAX_OUTPUT: usize = 16 * 1024 * 1024;
const MAX_TEXT: usize = 5 * 1024 * 1024;
const ARTIFACTS: &str = include_str!("../../../scripts/rust/pdf-worker/artifacts.json");

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultBody {
    contract_version: &'static str,
    input_sha256: String,
    status: &'static str,
    error_code: Option<&'static str>,
    pages: Vec<Page>,
    provenance: serde_json::Value,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    page_number: usize,
    width: f32,
    height: f32,
    rotation: u16,
    text: String,
    layout: Vec<Item>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    text: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    font: String,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn rounded(n: f32) -> Result<f32, &'static str> {
    if !n.is_finite() {
        return Err("unsupported_pdf");
    }
    let n = (n * 1000.0).round() / 1000.0;
    if !n.is_finite() {
        return Err("unsupported_pdf");
    }
    Ok(if n == 0.0 { 0.0 } else { n })
}
fn limits() -> Result<(), &'static str> {
    use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
    for (resource, bound) in [(Resource::Cpu, 15), (Resource::Core, 0)] {
        let old = getrlimit(resource);
        let value = old.maximum.map_or(bound, |n| n.min(bound));
        setrlimit(
            resource,
            Rlimit {
                current: Some(value),
                maximum: Some(value),
            },
        )
        .map_err(|_| "os_limits_unavailable")?;
    }
    // Darwin rejects this address-space cap on the qualified arm64 host. This bootstrap
    // explicitly reports the missing hard memory isolation; Linux enforces RLIMIT_AS.
    #[cfg(target_os = "linux")]
    setrlimit(
        Resource::As,
        Rlimit {
            current: Some(512 * 1024 * 1024),
            maximum: Some(512 * 1024 * 1024),
        },
    )
    .map_err(|_| "os_limits_unavailable")?;
    // Independent wall deadline includes stdin. The supervisor must also enforce kill/reap.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(15));
        std::process::exit(124);
    });
    Ok(())
}
fn artifact() -> Result<serde_json::Value, &'static str> {
    let name = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "pdfium-mac-arm64.tgz",
        ("macos", "x86_64") => "pdfium-mac-x64.tgz",
        ("linux", "aarch64") => "pdfium-linux-arm64.tgz",
        ("linux", "x86_64") => "pdfium-linux-x64.tgz",
        _ => return Err("platform_unavailable"),
    };
    let manifest: serde_json::Value =
        serde_json::from_str(ARTIFACTS).map_err(|_| "artifact_manifest_invalid")?;
    manifest["artifacts"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["archive"] == name))
        .cloned()
        .ok_or("artifact_manifest_invalid")
}
fn bundled_library(pin: &serde_json::Value) -> Result<PathBuf, &'static str> {
    let library = pin["library"].as_str().ok_or("artifact_manifest_invalid")?;
    let path = std::env::current_exe()
        .map_err(|_| "artifact_unavailable")?
        .parent()
        .ok_or("artifact_unavailable")?
        .join("pdfium")
        .join(library);
    let meta = std::fs::symlink_metadata(&path).map_err(|_| "artifact_unavailable")?;
    if !meta.is_file() || meta.len() > 128 * 1024 * 1024 {
        return Err("artifact_unavailable");
    }
    let bytes = std::fs::read(&path).map_err(|_| "artifact_unavailable")?;
    if Some(hash(&bytes).as_str()) != pin["librarySha256"].as_str() {
        return Err("artifact_hash_mismatch");
    }
    Ok(path)
}
fn parse(bytes: &[u8], pin: &serde_json::Value) -> Result<Vec<Page>, &'static str> {
    let path = bundled_library(pin)?;
    let bindings = Pdfium::bind_to_library(path).map_err(|_| "artifact_unavailable")?;
    let pdfium = Pdfium::new(bindings);
    let doc = pdfium
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|error| match error {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
                "encrypted_pdf"
            }
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
                "malformed_pdf"
            }
            _ => "unsupported_pdf",
        })?;
    if doc.pages().len() > 200 {
        return Err("page_limit");
    }
    let mut pages = Vec::new();
    let mut total_items = 0;
    let mut total_text = 0;
    for (i, page) in doc.pages().iter().enumerate() {
        let text_page = page.text().map_err(|_| "unsupported_pdf")?;
        let chars = text_page.chars();
        if chars.len() > 25_000 || total_items + chars.len() > 200_000 {
            return Err("item_limit");
        }
        let mut layout = Vec::new();
        let mut raw = String::new();
        for ch in chars.iter() {
            let scalar = ch
                .unicode_char()
                .filter(|c| *c != '\0' && *c != '\u{fffd}')
                .ok_or("unsupported_pdf")?;
            raw.push(scalar);
            // PDFium inserts CR/LF as generated layout separators, not physical glyphs.
            if scalar == '\r' || scalar == '\n' {
                continue;
            }
            let bounds = ch.loose_bounds().map_err(|_| "unsupported_pdf")?;
            layout.push(Item {
                text: scalar.to_string(),
                x: rounded(bounds.left().value)?,
                y: rounded(bounds.bottom().value)?,
                width: rounded(bounds.width().value)?,
                height: rounded(bounds.height().value)?,
                font: ch.font_name(),
            });
        }
        // Matches source newline/NFC normalization; segmentation/geometry remain PDFium-specific.
        let text: String = raw
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .nfc()
            .collect();
        total_text += text.encode_utf16().count();
        total_items += layout.len();
        if total_text > MAX_TEXT {
            return Err("text_limit");
        }
        let width = rounded(page.width().value)?;
        let height = rounded(page.height().value)?;
        if width <= 0.0 || height <= 0.0 {
            return Err("unsupported_pdf");
        }
        pages.push(Page {
            page_number: i + 1,
            width,
            height,
            rotation: page.rotation().map_err(|_| "unsupported_pdf")?.as_degrees() as u16,
            text,
            layout,
        });
    }
    Ok(pages)
}
fn run() -> Result<(), &'static str> {
    if std::env::args_os().len() != 1 {
        return Err("arguments_refused");
    }
    limits()?;
    let mut bytes = Vec::new();
    io::stdin()
        .take((MAX_INPUT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "input_unavailable")?;
    if bytes.is_empty() || bytes.len() > MAX_INPUT {
        return Err("input_limit");
    }
    let pin = artifact()?;
    let mut result = ResultBody {
        contract_version: "document-pdf-text-pdfium-v1",
        input_sha256: hash(&bytes),
        status: "ok",
        error_code: None,
        pages: Vec::new(),
        provenance: serde_json::json!({"parserId":"pdfium", "parserVersion":format!("151.0.7881.0+binding0.9.4.adapter1.{}", &pin["librarySha256"].as_str().ok_or("artifact_manifest_invalid")?[..16]), "bindingVersion":"0.9.4", "adapterVersion":"1", "artifactSha256":pin["sha256"],"librarySha256":pin["librarySha256"],"layoutConvention":"pdfium-character-loose-box-bottom-left-v1", "limits":{"cpuSeconds":15,"addressSpaceBytes":if cfg!(target_os="linux") { Some(536870912_u64) } else { None },"wallSeconds":15,"macMemoryEnforcementQualified":false}}),
    };
    match parse(&bytes, &pin) {
        Ok(pages) => {
            if pages.iter().all(|p| p.layout.is_empty()) {
                result.status = "no_text_layer";
                result.error_code = Some("no_text_layer");
            }
            result.pages = pages;
        }
        Err(code @ ("malformed_pdf" | "encrypted_pdf" | "unsupported_pdf")) => {
            result.status = code;
            result.error_code = Some(code);
        }
        Err(code @ ("page_limit" | "item_limit" | "text_limit")) => {
            result.status = "resource_limit";
            result.error_code = Some(code);
        }
        Err(code) => return Err(code),
    }
    let output = serde_json::to_vec(&result).map_err(|_| "protocol_unavailable")?;
    if output.len() + 1 > MAX_OUTPUT {
        return Err("stdout_limit");
    }
    let mut out = io::stdout().lock();
    out.write_all(&output)
        .and_then(|_| out.write_all(b"\n"))
        .map_err(|_| "output_unavailable")
}
fn main() {
    if let Err(code) = run() {
        eprintln!("pdf_worker:{code}");
        std::process::exit(1);
    }
}
