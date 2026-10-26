//! Process-registered task maintenance with transient current service credentials, never local grants.
use crate::backend::Backend;
use rentemester_application::tasks::{Action, Authority};
use rentemester_runtime::auth::{Clock, SystemClock};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
pub fn validate_flags(
    worker: bool,
    stdin: bool,
    configured: bool,
    exclusive: bool,
) -> Result<(), &'static str> {
    if worker != stdin || worker && (!configured || exclusive) {
        return Err(
            "task worker requires paired --task-worker --task-service-key-stdin, --config and ordinary reopen",
        );
    }
    Ok(())
}
pub struct Worker {
    backend: Arc<Backend>,
    task: tokio::task::JoinHandle<()>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
}
impl Worker {
    pub fn start(backend: Arc<Backend>) -> Self {
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let b = backend.clone();
        let task = tokio::spawn(async move {
            crate::shutdown::run_worker(stopped, Duration::from_secs(60), move |_| {
                let b = b.clone();
                tokio::task::spawn_blocking(move || pass(&b))
            })
            .await;
        });
        Self {
            backend,
            task,
            stop: Some(stop),
        }
    }
    pub fn take_stop_signal(&mut self) -> Option<tokio::sync::oneshot::Sender<()>> {
        self.stop.take()
    }
    pub async fn stop(mut self) {
        self.backend
            .task_runtime_live
            .store(false, Ordering::Release);
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let _ = (&mut self.task).await;
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.backend
            .task_runtime_live
            .store(false, Ordering::Release);
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.task.abort();
    }
}
fn pass(backend: &Backend) -> Result<usize, ()> {
    let result = (|| {
        let key = backend.task_executor.as_deref().ok_or(())?;
        let lease = rentemester_runtime::auth::new_token().map_err(|_| ())?;
        let lease = lease.as_str();
        let mut owner = backend.owner.lock().map_err(|_| ())?;
        let claim = owner
            .tasks(
                &SystemClock,
                Authority::Service(key),
                Action::ClaimRuntime { owner: lease },
                None,
                None,
            )
            .map_err(|_| ())?;
        if claim.retired {
            return Err(());
        }
        let claim = claim.outcome.map_err(|_| ())?;
        if claim["acquired"] != true {
            return Ok(false);
        }
        let input_key = format!("task-tick:{lease}");
        let run = owner.tasks(
            &SystemClock,
            Authority::Service(key),
            Action::Maintain { owner: lease },
            Some(&input_key),
            None,
        );
        match run {
            Ok(a) if !a.retired && a.outcome.is_ok() => Ok(true),
            _ => {
                let _ = owner.fail_task_runtime(&SystemClock, lease);
                Err(())
            }
        }
    })();
    if let Ok(true) = result {
        backend.task_runtime_live.store(true, Ordering::Release);
    } else if result.is_err() {
        backend.task_runtime_live.store(false, Ordering::Release);
    }
    if result.is_err() {
        eprintln!("[tasks] Runtimekørsel fejlede; se status i Rentemester.");
    }
    result.map(|_| 0)
}
/// Persisted heartbeat alone is never evidence of a registered live worker.
pub fn metadata(backend: &Backend, value: &mut serde_json::Value) {
    let Some(runtime) = value.get_mut("runtime") else {
        return;
    };
    let live = backend.task_runtime_live.load(Ordering::Acquire)
        && runtime["lastError"].is_null()
        && runtime["lastTickAt"]
            .as_str()
            .and_then(|s| rentemester_engine::tasks::instant(s).ok())
            .zip(SystemClock.now_unix_ms().ok())
            .is_some_and(|((_, at), now)| {
                let age = i128::from(now) - at;
                (0..120_000).contains(&age)
            });
    runtime["running"] = serde_json::json!(live);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_paired_transient_credentials_and_ordinary_reopen_only() {
        for flags in [
            (true, false, true, false),
            (false, true, true, false),
            (true, true, false, false),
            (true, true, true, true),
        ] {
            assert!(validate_flags(flags.0, flags.1, flags.2, flags.3).is_err());
        }
        assert!(validate_flags(true, true, true, false).is_ok());
        assert!(validate_flags(false, false, false, false).is_ok());
    }
}
