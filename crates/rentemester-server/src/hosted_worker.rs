//! Explicit process-owned worker. No cookie, persisted key, background account or second opener.
use crate::backend::Backend;
use rentemester_runtime::{auth::SystemClock, service_key::ServiceKey};
use std::{io::Read, sync::Arc, time::Duration};
use zeroize::Zeroizing;
pub const MAX_CANDIDATES: usize = 32;
const INTERVAL: Duration = Duration::from_secs(30);

pub fn validate_flags(
    worker: bool,
    stdin: bool,
    configured: bool,
    exclusive: bool,
) -> Result<(), &'static str> {
    if worker != stdin || worker && (!configured || exclusive) {
        return Err(
            "scheduled worker requires paired --scheduled-worker --scheduled-service-key-stdin, --config, and ordinary reopen (no create/restore/verify)",
        );
    }
    Ok(())
}
/// Bounded plaintext exists only in zeroizing memory; no read of an operator-selected file.
pub fn read_key(reader: impl Read, workspace: &str) -> Result<ServiceKey, &'static str> {
    let mut bytes = Zeroizing::new(Vec::new());
    reader
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| "scheduled service key input refused")?;
    if bytes.len() > 4096 {
        return Err("scheduled service key input refused");
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "scheduled service key input refused")?;
    ServiceKey::new(workspace, text).map_err(|_| "scheduled service key input refused")
}

pub struct Worker {
    task: tokio::task::JoinHandle<()>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
}
impl Worker {
    pub fn start(backend: Arc<Backend>) -> Self {
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut cursor = 0usize;
            loop {
                // Exactly one pass is outstanding; no interval backlog after sleep.
                let native = backend.clone();
                match tokio::task::spawn_blocking(move || pass(&native, cursor)).await {
                    Ok(Ok(next)) => cursor = next,
                    // A poisoned/retired owner or uncertain outcome is never retried by this worker.
                    _ => break,
                }
                tokio::select! {_ = &mut stopped=>break,_=tokio::time::sleep(INTERVAL)=>{}}
            }
        });
        Self {
            task,
            stop: Some(stop),
        }
    }
    pub async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let _ = (&mut self.task).await;
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.task.abort();
    }
}

fn pass(backend: &Backend, cursor: usize) -> Result<usize, ()> {
    let key = backend.scheduled_executor.as_deref().ok_or(())?;
    let catalog = backend
        .owner
        .lock()
        .map_err(|_| ())?
        .catalog()
        .map_err(|_| ())?;
    let active = catalog
        .companies
        .into_iter()
        .filter(|c| !c.archived)
        .collect::<Vec<_>>();
    if active.is_empty() {
        return Ok(0);
    }
    let mut candidates = Vec::new();
    let mut inspected = 0;
    for offset in 0..active.len().min(MAX_CANDIDATES) {
        let company = &active[(cursor + offset) % active.len()];
        inspected += 1;
        let mut owner = backend.lock_company(&company.slug)?;
        let attempt = owner
            .hosted_recurring_candidates(key, &SystemClock, MAX_CANDIDATES - candidates.len())
            .map_err(|_| ())?;
        if attempt.retired {
            return Err(());
        }
        match attempt.outcome {
            Ok(value) => {
                let rows = value.value["rows"].as_array().ok_or(())?;
                if rows.len() > MAX_CANDIDATES - candidates.len() {
                    return Err(());
                }
                for row in rows {
                    if row["companySlug"] != company.slug {
                        return Err(());
                    }
                    let id = row["templateId"].as_i64().filter(|id| *id > 0).ok_or(())?;
                    candidates.push((company.slug.clone(), id));
                }
            }
            Err(f)
                if f.stage
                    == rentemester_application::control_fence::FailureStage::NotCommitted => {}
            Err(_) => return Err(()),
        }
        if candidates.len() == MAX_CANDIDATES {
            break;
        }
    }
    for (slug, id) in candidates {
        let mut owner = backend.lock_company(&slug)?;
        let attempt = owner
            .dispatch_hosted_recurring(key, &SystemClock, id)
            .map_err(|_| ())?;
        if attempt.retired {
            return Err(());
        }
        if let Err(f) = attempt.outcome {
            use rentemester_application::service_company::{BusinessStage, UsageStage};
            if f.usage == UsageStage::OutcomeUnknown || f.business == BusinessStage::OutcomeUnknown
            {
                return Err(());
            }
            // Known refusal leaves work untouched. Current authority is checked again on any later due read.
        }
    }
    Ok((cursor + inspected) % active.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flags_are_explicit_paired_and_exclude_other_operator_commands() {
        assert!(validate_flags(false, false, false, false).is_ok());
        assert!(validate_flags(true, true, true, false).is_ok());
        for args in [
            (true, false, true, false),
            (false, true, true, false),
            (true, true, false, false),
            (true, true, true, true),
        ] {
            assert!(validate_flags(args.0, args.1, args.2, args.3).is_err());
        }
    }
    #[test]
    fn key_input_is_bounded_single_secret_and_not_echoed() {
        assert!(read_key(&b"synthetic-private-key\n"[..], "w").is_ok());
        for value in [
            vec![],
            vec![b'x'; 4097],
            b"secret\nsecond".to_vec(),
            b"secret\r\n".to_vec(),
            vec![255],
        ] {
            assert_eq!(
                read_key(value.as_slice(), "w").err(),
                Some("scheduled service key input refused")
            );
        }
    }
}
