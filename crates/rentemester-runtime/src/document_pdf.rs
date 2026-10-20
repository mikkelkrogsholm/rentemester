//! Fixed bundled offline PDF subprocess. No workspace/database/network or caller-selected command.
use rentemester_engine::document_pdf::{ParseResult, ParserIdentity};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};
const MAX_INPUT: usize = 10 * 1024 * 1024;
const MAX_STDOUT: usize = 16 * 1024 * 1024;
const MAX_STDERR: usize = 64 * 1024;
const DEADLINE: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(10);
const MAX_RSS: u64 = 512 * 1024 * 1024;
const MANIFEST: &str = include_str!("../../../scripts/rust/pdf-worker/artifacts.json");

/// Curated operational causes only. Child stderr and operator paths never cross this API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationalError {
    Unavailable,
    Timeout,
    StdoutLimit,
    StderrLimit,
    Crashed,
    ProtocolViolation,
    MemoryLimit,
}
impl OperationalError {
    pub fn code(self) -> rentemester_engine::document_pdf::OperationalError {
        use rentemester_engine::document_pdf::OperationalError as Code;
        match self {
            Self::Unavailable => Code::WorkerUnavailable,
            Self::Timeout => Code::Timeout,
            Self::StdoutLimit => Code::StdoutLimit,
            Self::StderrLimit => Code::StderrLimit,
            Self::Crashed => Code::Crashed,
            Self::MemoryLimit => Code::MemoryLimit,
            Self::ProtocolViolation => Code::ProtocolViolation,
        }
    }
}
impl std::fmt::Display for OperationalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PDF parser {:?}", self)
    }
}
impl std::error::Error for OperationalError {}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Artifact {
    archive: String,
    #[serde(rename = "url")]
    _url: String,
    sha256: String,
    library: String,
    library_sha256: String,
}
#[derive(Deserialize)]
struct Manifest {
    artifacts: Vec<Artifact>,
    #[serde(flatten)]
    _metadata: serde_json::Map<String, Value>,
}

/// An explicit private operator bundle; startup pins its executable hash and native artifact.
/// Clone is cheap and does not start a process. No path is accepted from public requests.
#[derive(Clone)]
pub struct Config {
    worker: PathBuf,
    worker_hash: String,
    library: PathBuf,
    pin: Artifact,
    identity: ParserIdentity,
}
impl Config {
    pub fn new(bundle_dir: PathBuf) -> Result<Self, OperationalError> {
        if !bundle_dir.is_absolute() {
            return Err(OperationalError::Unavailable);
        }
        let root = bundle_dir
            .canonicalize()
            .map_err(|_| OperationalError::Unavailable)?;
        let pin = platform_artifact()?;
        let worker = root.join("rentemester-pdf-worker");
        let library = root.join("pdfium").join(&pin.library);
        let worker_hash = file_hash(&worker, 128 * 1024 * 1024, true)?;
        if file_hash(&library, 128 * 1024 * 1024, false)? != pin.library_sha256 {
            return Err(OperationalError::Unavailable);
        }
        let identity = ParserIdentity {
            parser_id: "pdfium".into(),
            parser_version: format!(
                "151.0.7881.0+binding0.9.4.adapter1.{}",
                &pin.library_sha256[..16]
            ),
            contract_version: "document-pdf-text-pdfium-v1".into(),
        };
        identity
            .validate()
            .map_err(|_| OperationalError::Unavailable)?;
        Ok(Self {
            worker,
            worker_hash,
            library,
            pin,
            identity,
        })
    }
    pub fn identity(&self) -> &ParserIdentity {
        &self.identity
    }
    pub fn parse(&self, bytes: &[u8]) -> Result<ParseResult, OperationalError> {
        self.parse_limited(bytes, DEADLINE, MAX_RSS)
    }
    fn parse_limited(
        &self,
        bytes: &[u8],
        deadline: Duration,
        rss: u64,
    ) -> Result<ParseResult, OperationalError> {
        if bytes.is_empty() || bytes.len() > MAX_INPUT {
            return Err(OperationalError::ProtocolViolation);
        }
        let started = Instant::now();
        // Recheck admitted bundle before every launch, not just operator startup.
        if file_hash(&self.worker, 128 * 1024 * 1024, true)? != self.worker_hash
            || file_hash(&self.library, 128 * 1024 * 1024, false)? != self.pin.library_sha256
        {
            return Err(OperationalError::Unavailable);
        }
        let raw = execute(&self.worker, bytes, started, deadline, rss)?;
        decode(&raw, bytes, &self.identity, &self.pin)
    }
}
fn platform_artifact() -> Result<Artifact, OperationalError> {
    let archive = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "pdfium-mac-arm64.tgz",
        ("macos", "x86_64") => "pdfium-mac-x64.tgz",
        ("linux", "aarch64") => "pdfium-linux-arm64.tgz",
        ("linux", "x86_64") => "pdfium-linux-x64.tgz",
        _ => return Err(OperationalError::Unavailable),
    };
    serde_json::from_str::<Manifest>(MANIFEST)
        .map_err(|_| OperationalError::Unavailable)?
        .artifacts
        .into_iter()
        .find(|p| p.archive == archive)
        .ok_or(OperationalError::Unavailable)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn file_hash(path: &Path, max: u64, executable: bool) -> Result<String, OperationalError> {
    // Resolve no symlink below the operator-selected bundle root (canonicalized at admission).
    for parent in path.ancestors().take(3) {
        let meta = std::fs::symlink_metadata(parent).map_err(|_| OperationalError::Unavailable)?;
        if meta.file_type().is_symlink() {
            return Err(OperationalError::Unavailable);
        }
    }
    let meta = std::fs::symlink_metadata(path).map_err(|_| OperationalError::Unavailable)?;
    if !meta.is_file() || meta.len() > max || (executable && meta.permissions().mode() & 0o111 == 0)
    {
        return Err(OperationalError::Unavailable);
    }
    let mut file = File::open(path).map_err(|_| OperationalError::Unavailable)?;
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buf = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|_| OperationalError::Unavailable)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max {
            return Err(OperationalError::Unavailable);
        }
        digest.update(&buf[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

struct Running {
    child: Child,
    stopped: bool,
}
impl Running {
    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        // Only the process group created for this exact child. Also closes descendant pipes.
        if let Some(pid) = rustix::process::Pid::from_raw(self.child.id() as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
    }
}
fn pipe_read(
    mut pipe: impl Read,
    max: usize,
    flag: u8,
    flags: &AtomicU8,
) -> Result<Vec<u8>, OperationalError> {
    let mut data = Vec::new();
    let result = pipe.by_ref().take((max + 1) as u64).read_to_end(&mut data);
    if data.len() > max {
        flags.fetch_or(flag, Ordering::SeqCst);
        return Err(if flag == 1 {
            OperationalError::StdoutLimit
        } else {
            OperationalError::StderrLimit
        });
    }
    result.map_err(|_| OperationalError::Crashed)?;
    Ok(data)
}
fn execute(
    worker: &Path,
    bytes: &[u8],
    started: Instant,
    deadline: Duration,
    max_rss: u64,
) -> Result<Vec<u8>, OperationalError> {
    let child = Command::new(worker)
        .env_clear()
        .current_dir("/")
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| OperationalError::Unavailable)?;
    let mut running = Running {
        child,
        stopped: false,
    };
    let stdin = running
        .child
        .stdin
        .take()
        .ok_or(OperationalError::Crashed)?;
    let stdout = running
        .child
        .stdout
        .take()
        .ok_or(OperationalError::Crashed)?;
    let stderr = running
        .child
        .stderr
        .take()
        .ok_or(OperationalError::Crashed)?;
    let flags = Arc::new(AtomicU8::new(0));
    std::thread::scope(|scope| {
        let writer = scope.spawn(move || {
            let mut stdin = stdin;
            stdin.write_all(bytes)
        });
        let output = scope.spawn(|| pipe_read(stdout, MAX_STDOUT, 1, &flags));
        let diagnostic = scope.spawn(|| pipe_read(stderr, MAX_STDERR, 2, &flags));
        let outcome = loop {
            let flags = flags.load(Ordering::SeqCst);
            if flags & 1 != 0 {
                break Err(OperationalError::StdoutLimit);
            }
            if flags & 2 != 0 {
                break Err(OperationalError::StderrLimit);
            }
            if started.elapsed() >= deadline {
                break Err(OperationalError::Timeout);
            }
            match running.child.try_wait() {
                Ok(Some(status)) => {
                    break if status.success() {
                        Ok(())
                    } else if status.code() == Some(124) {
                        Err(OperationalError::Timeout)
                    } else {
                        Err(OperationalError::Crashed)
                    };
                }
                Err(_) => break Err(OperationalError::Crashed),
                Ok(None) => {}
            }
            match resident_bytes(running.child.id()) {
                Ok(value) if value > max_rss => break Err(OperationalError::MemoryLimit),
                Ok(_) => {}
                Err(_) => match running.child.try_wait() {
                    Ok(Some(status)) => {
                        break if status.success() {
                            Ok(())
                        } else {
                            Err(OperationalError::Crashed)
                        };
                    }
                    _ => break Err(OperationalError::Unavailable),
                },
            }
            std::thread::sleep(POLL);
        };
        running.stop();
        let write_result = writer.join().map_err(|_| OperationalError::Crashed)?;
        let out = output.join().map_err(|_| OperationalError::Crashed)?;
        let err = diagnostic.join().map_err(|_| OperationalError::Crashed)?;
        // No diagnostic bytes are returned, logged, or used as trusted evidence.
        outcome?;
        write_result.map_err(|_| OperationalError::Crashed)?;
        let _ = err?;
        out
    })
}
#[cfg(target_os = "macos")]
fn resident_bytes(pid: u32) -> Result<u64, OperationalError> {
    libproc::libproc::proc_pid::pidinfo::<libproc::libproc::task_info::TaskInfo>(pid as i32, 0)
        .map(|info| info.pti_resident_size)
        .map_err(|_| OperationalError::Unavailable)
}
#[cfg(target_os = "linux")]
fn resident_bytes(pid: u32) -> Result<u64, OperationalError> {
    let path = format!("/proc/{pid}/status");
    let mut status = String::new();
    File::open(path)
        .map_err(|_| OperationalError::Unavailable)?
        .take(65536)
        .read_to_string(&mut status)
        .map_err(|_| OperationalError::Unavailable)?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse::<u64>().ok())
        .and_then(|n| n.checked_mul(1024))
        .ok_or(OperationalError::Unavailable)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    contract_version: String,
    input_sha256: String,
    status: rentemester_engine::document_pdf::Status,
    error_code: Option<rentemester_engine::document_pdf::ErrorCode>,
    pages: Vec<rentemester_engine::document_pdf::Page>,
    provenance: Provenance,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Provenance {
    parser_id: String,
    parser_version: String,
    binding_version: String,
    adapter_version: String,
    artifact_sha256: String,
    library_sha256: String,
    layout_convention: String,
    limits: Value,
}
fn decode(
    raw: &[u8],
    source: &[u8],
    identity: &ParserIdentity,
    pin: &Artifact,
) -> Result<ParseResult, OperationalError> {
    if raw.len() > MAX_STDOUT {
        return Err(OperationalError::StdoutLimit);
    }
    let envelope: Envelope =
        serde_json::from_slice(raw).map_err(|_| OperationalError::ProtocolViolation)?;
    let p = envelope.provenance;
    let expected_limits = serde_json::json!({"cpuSeconds":15,"addressSpaceBytes":if cfg!(target_os="linux") {Some(536870912_u64)}else{None},"wallSeconds":15,"macMemoryEnforcementQualified":false});
    if p.parser_id != identity.parser_id
        || p.parser_version != identity.parser_version
        || p.binding_version != "0.9.4"
        || p.adapter_version != "1"
        || p.artifact_sha256 != pin.sha256
        || p.library_sha256 != pin.library_sha256
        || p.layout_convention != "pdfium-character-loose-box-bottom-left-v1"
        || p.limits != expected_limits
    {
        return Err(OperationalError::ProtocolViolation);
    }
    let result = ParseResult {
        contract_version: envelope.contract_version,
        input_sha256: envelope.input_sha256,
        status: envelope.status,
        error_code: envelope.error_code,
        pages: envelope.pages,
    };
    result
        .validate(&hash(source), identity)
        .map_err(|_| OperationalError::ProtocolViolation)?;
    Ok(result)
}
#[cfg(test)]
mod tests;
