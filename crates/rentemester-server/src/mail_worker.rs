//! Explicit receive-only worker. Transport runs outside the Owner and database locks.
use crate::backend::Backend;
use rentemester_runtime::{auth::SystemClock, imap::{Client, NetworkClient}};
use std::{sync::Arc, time::Duration};
const MAX_COMPANIES: usize = 32;

pub fn validate_flags(worker: bool, stdin: bool, configured: bool, exclusive: bool) -> Result<(), &'static str> {
    if worker != stdin || worker && (!configured || exclusive) {
        return Err("mail worker requires paired --mail-worker --mail-service-key-stdin, --config, and ordinary reopen (no create/restore/verify)");
    }
    Ok(())
}
pub struct Worker {
    task: tokio::task::JoinHandle<()>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
}
impl Worker {
    pub fn start(backend: Arc<Backend>) -> Self {
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            crate::shutdown::run_worker(stopped, Duration::from_secs(30), move |cursor| {
                let native = backend.clone();
                tokio::task::spawn_blocking(move || pass(&native, cursor))
            }).await;
        });
        Self { task, stop: Some(stop) }
    }
    pub fn take_stop_signal(&mut self) -> Option<tokio::sync::oneshot::Sender<()>> {
        self.stop.take()
    }
    pub async fn stop(mut self) {
        if let Some(stop) = self.stop.take() { let _ = stop.send(()); }
        let _ = (&mut self.task).await;
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() { let _ = stop.send(()); }
        self.task.abort();
    }
}
fn pass(backend: &Backend, cursor: usize) -> Result<usize, ()> {
    let key = backend.mail_executor.as_deref().ok_or(())?;
    let catalog = backend.owner.lock().map_err(|_| ())?.catalog().map_err(|_| ())?;
    let companies = catalog.companies.into_iter().filter(|c| !c.archived).collect::<Vec<_>>();
    if companies.is_empty() { return Ok(0); }
    let count = companies.len().min(MAX_COMPANIES);
    for offset in 0..count {
        let company = &companies[(cursor + offset) % companies.len()];
        let admission = {
            let mut owner = backend.lock_company(&company.slug)?;
            let attempt = owner.prepare_service_mail_poll(key, &SystemClock).map_err(|_| ())?;
            if attempt.retired { return Err(()); }
            match attempt.outcome {
                Ok(value) => value.value,
                Err(f) if f.stage == rentemester_application::control_fence::FailureStage::NotCommitted => continue,
                Err(_) => return Err(()),
            }
        };
        // No Owner guard, transaction, or saved human credential is held during network I/O.
        let mut client = NetworkClient::new(backend.imap_transport.clone()).map_err(|_| ())?;
        let outcome = client.fetch(admission.config(), admission.cursor());
        let mut owner = backend.lock_company(&company.slug)?;
        let attempt = owner.finish_service_mail_poll(key, &SystemClock, admission, outcome).map_err(|_| ())?;
        if attempt.retired { return Err(()); }
        if let Err(f) = attempt.outcome
            && f.stage != rentemester_application::control_fence::FailureStage::NotCommitted {
            return Err(());
        }
    }
    Ok((cursor + count) % companies.len())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mail_worker_requires_explicit_reopen_and_both_flags() {
        assert!(validate_flags(false,false,false,false).is_ok());
        assert!(validate_flags(true,true,true,false).is_ok());
        for (w,s,c,e) in [(true,false,true,false),(false,true,true,false),(true,true,false,false),(true,true,true,true)] {
            assert!(validate_flags(w,s,c,e).is_err());
        }
    }
}
