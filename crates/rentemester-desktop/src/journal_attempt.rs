//! One durable, origin/company/principal-bound attempt; never an automatic queue.
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
#[derive(Clone)]
pub struct Attempt {
    pub origin: String,
    pub company: String,
    pub principal: String,
    pub body: Vec<u8>,
    pub ledger_uuid: String,
    pub local_identity: Option<String>,
}
pub fn directory() -> PathBuf {
    std::env::var_os("RENTEMESTER_DESKTOP_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join("Library/Application Support/Rentemester Desktop")
        })
}
fn private(path: &Path, directory: bool) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(|_| "Den private posteringsfil kunne ikke åbnes")?;
    if m.file_type().is_symlink()
        || (directory && !m.is_dir())
        || (!directory && !m.is_file())
        || m.mode() & 0o077 != 0
    {
        return Err(
            "Posteringsstatus skal være en privat mappe0700/fil0600 uden symbolske links.".into(),
        );
    }
    let parent = m.uid();
    let me = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|_| "Brugeridentitet kunne ikke læses")?;
    if !me.status.success()
        || String::from_utf8_lossy(&me.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            != Some(parent)
    {
        return Err("Posteringsstatus tilhører en anden bruger.".into());
    }
    Ok(())
}
impl Attempt {
    fn record(&self) -> Result<Value, String> {
        Ok(
            json!({"format":"rentemester-desktop-journal/1","origin":self.origin,"company":self.company,"principal":self.principal,"ledgerUuid":self.ledger_uuid,"localIdentity":self.local_identity,"body":String::from_utf8(self.body.clone()).map_err(|_|"Ugyldig posteringsanmodning")?}),
        )
    }
    pub fn can_rebind(
        &self,
        company: &str,
        principal: &str,
        ledger: &str,
        local: Option<&str>,
    ) -> bool {
        self.company == company
            && self.principal == principal
            && self.ledger_uuid == ledger
            && self
                .local_identity
                .as_deref()
                .is_some_and(|old| Some(old) == local)
    }
    /// Only called after the user reviews old/new endpoints and explicitly confirms.
    pub fn rebind(
        &self,
        dir: &Path,
        origin: &str,
        company: &str,
        principal: &str,
        ledger: &str,
        local: Option<&str>,
    ) -> Result<Self, String> {
        if !self.can_rebind(company, principal, ledger, local) {
            return Err("Den lokale virksomhed kunne ikke identificeres sikkert. Det oprindelige forsøg bevares.".into());
        }
        let current = Self::load(dir)?.ok_or("Det gemte forsøg mangler")?;
        if current.body != self.body
            || current.ledger_uuid != self.ledger_uuid
            || current.local_identity != self.local_identity
            || !current.matches(&self.origin, &self.company, &self.principal)
        {
            return Err("Det gemte forsøg har ændret sig".into());
        }
        let mut next = self.clone();
        next.origin = origin.to_owned();
        let temporary = dir.join("pending-journal-rebind.json");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| "Et tidligere endpoints-skift er uafklaret; originalen bevares")?;
        file.write_all(next.record()?.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Endpoints-skift kunne ikke gemmes")?;
        fs::rename(temporary, dir.join("pending-journal.json"))
            .map_err(|_| "Endpoints-skift kunne ikke færdiggøres")?;
        std::fs::File::open(dir)
            .and_then(|f| f.sync_all())
            .map_err(|_| "Endpoints-skift kunne ikke synkroniseres")?;
        Ok(next)
    }
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        if !dir.is_absolute() {
            return Err("Posteringsstatus kræver en absolut datamappe.".into());
        }
        if !dir.exists() {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(dir)
                .map_err(|_| "Den private statusmappe kunne ikke oprettes")?;
        }
        private(dir, true)?;
        if dir
            .canonicalize()
            .map_err(|_| "Statusmappen kunne ikke åbnes")?
            != dir
        {
            return Err("Statusmappen må ikke gå gennem symbolske links.".into());
        }
        let record = self.record()?;
        let mut f=OpenOptions::new().write(true).create_new(true).mode(0o600).open(dir.join("pending-journal.json")).map_err(|_|"En uafsluttet postering findes allerede. Åbn den oprindelige virksomhed og afklar forsøget først.")?;
        f.write_all(record.to_string().as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(|_| {
                "Posteringsstatus kunne ikke gemmes; der er ikke sendt en postering.".to_owned()
            })?;
        std::fs::File::open(dir)
            .and_then(|f| f.sync_all())
            .map_err(|_| "Posteringsstatus kunne ikke synkroniseres")?;
        Ok(())
    }
    pub fn load(dir: &Path) -> Result<Option<Self>, String> {
        let p = dir.join("pending-journal.json");
        if !p
            .try_exists()
            .map_err(|_| "Posteringsstatus kunne ikke kontrolleres")?
        {
            return Ok(None);
        }
        private(dir, true)?;
        private(&p, false)?;
        let mut bytes = Vec::new();
        std::fs::File::open(p)
            .and_then(|f| f.take(131073).read_to_end(&mut bytes))
            .map_err(|_| "Posteringsstatus kunne ikke læses")?;
        if bytes.len() > 131072 {
            return Err("Posteringsstatus overskrider grænsen".into());
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| "Posteringsstatus er ugyldig")?;
        let text = |key| {
            v[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "Posteringsstatus mangler felter".to_owned())
        };
        if v["format"] != "rentemester-desktop-journal/1" {
            return Err("Ukendt posteringsstatus".into());
        }
        let result = Self {
            origin: text("origin")?,
            company: text("company")?,
            principal: text("principal")?,
            body: text("body")?.into_bytes(),
            ledger_uuid: text("ledgerUuid")?,
            local_identity: v["localIdentity"].as_str().map(str::to_owned),
        };
        let body: Value =
            serde_json::from_slice(&result.body).map_err(|_| "Ugyldig gemt postering")?;
        if body["confirm"] != true
            || body
                .pointer("/input/idempotencyKey")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
        {
            return Err("Gemt postering mangler nøgle eller bekræftelse".into());
        }
        Ok(Some(result))
    }
    pub fn matches(&self, origin: &str, company: &str, principal: &str) -> bool {
        self.origin == origin && self.company == company && self.principal == principal
    }
    pub fn finish(&self, dir: &Path) -> Result<(), String> {
        let Some(current) = Self::load(dir)? else {
            return Err("Posteringsstatus er forsvundet; behold kvitteringen.".into());
        };
        if !current.matches(&self.origin, &self.company, &self.principal)
            || current.body != self.body
            || current.ledger_uuid != self.ledger_uuid
            || current.local_identity != self.local_identity
        {
            return Err("Posteringsstatus har ændret sig; den bevares.".into());
        }
        fs::remove_file(dir.join("pending-journal.json")).map_err(|_| {
            "Bogført, men den lokale status kunne ikke afsluttes. Brug samme forsøg igen.".into()
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_body_survives_restart_and_cannot_be_overwritten_or_cross_bound() {
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("rm-desktop-attempt-{}", std::process::id()));
        let a = Attempt {
            origin: "http://127.0.0.1:2".into(),
            company: "synthetic".into(),
            ledger_uuid: "synthetic-ledger".into(),
            local_identity: Some("/private/synthetic-profile:workspace".into()),
            principal: "owner".into(),
            body: br#"{ "confirm":true,"input":{"idempotencyKey":"persistent-key"}}"#.to_vec(),
        };
        a.save(&dir).unwrap();
        assert!(a.save(&dir).is_err());
        let b = Attempt::load(&dir).unwrap().unwrap();
        assert_eq!(a.body, b.body);
        assert!(!b.matches(&a.origin, "different", &a.principal));
        assert!(!b.can_rebind(
            &b.company,
            &b.principal,
            "different-ledger",
            b.local_identity.as_deref()
        ));
        assert!(!b.can_rebind(
            &b.company,
            "different-user",
            &b.ledger_uuid,
            b.local_identity.as_deref()
        ));
        assert!(!b.can_rebind(&b.company, &b.principal, &b.ledger_uuid, None));
        let rebound = b
            .rebind(
                &dir,
                "http://127.0.0.1:3",
                &b.company,
                &b.principal,
                &b.ledger_uuid,
                b.local_identity.as_deref(),
            )
            .unwrap();
        assert_eq!(rebound.body, b.body);
        assert_eq!(
            Attempt::load(&dir).unwrap().unwrap().origin,
            "http://127.0.0.1:3"
        );
        rebound.finish(&dir).unwrap();
        assert!(Attempt::load(&dir).unwrap().is_none());
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn lost_response_keeps_exact_attempt_for_explicit_retry() {
        use rentemester_client::{ApiClient, Error, JournalMutation};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut received = Vec::new();
            for step in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 2048];
                loop {
                    let size = stream.read(&mut buffer).unwrap();
                    assert!(size > 0);
                    bytes.extend_from_slice(&buffer[..size]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|n| n.parse::<usize>().ok())
                            })
                            .unwrap();
                        if bytes.len() >= end + 4 + length {
                            received.push(bytes[end + 4..end + 4 + length].to_vec());
                            break;
                        }
                    }
                }
                if step == 1 {
                    let body = r#"{"ok":true,"entryId":42,"entryNo":"synthetic-42"}"#;
                    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
                }
                // First response deliberately disappears after receiving the request.
            }
            received
        });
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("rm-lost-post-response-{}", std::process::id()));
        let a = Attempt {
            origin: origin.clone(),
            company: "synthetic".into(),
            principal: "synthetic-owner".into(),
            ledger_uuid: "synthetic-ledger".into(),
            local_identity: None,
            body: br#"{ "confirm":true,"input":{"idempotencyKey":"one-exact-key"}}"#.to_vec(),
        };
        a.save(&dir).unwrap();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let client = {
            let _guard = runtime.enter();
            ApiClient::new(&origin, true, Some("synthetic=cookie".into())).unwrap()
        };
        assert_eq!(
            runtime.block_on(client.manual_journal(
                &a.company,
                JournalMutation::Post,
                a.body.clone()
            )),
            Err(Error::OutcomeUnknown)
        );
        let recovered = Attempt::load(&dir).unwrap().unwrap();
        assert_eq!(recovered.body, a.body);
        let result = runtime
            .block_on(client.manual_journal(
                &recovered.company,
                JournalMutation::Post,
                recovered.body.clone(),
            ))
            .unwrap();
        assert_eq!(result["entryId"], 42);
        let received = server.join().unwrap();
        assert_eq!(received, vec![a.body.clone(), a.body.clone()]);
        recovered.finish(&dir).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
