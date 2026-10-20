//! Process signals stop admission and drain HTTP and explicitly enabled workers.
/// One pass at a time, no interval backlog, and no retry of a failed or
/// uncertain pass. A stop waits for the admitted blocking pass to finish.
pub async fn run_worker(
    mut stopped: tokio::sync::oneshot::Receiver<()>,
    interval: std::time::Duration,
    mut start: impl FnMut(usize) -> tokio::task::JoinHandle<Result<usize, ()>>,
) {
    let mut cursor = 0;
    loop {
        if !matches!(
            stopped.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ) {
            break;
        }
        match start(cursor).await {
            Ok(Ok(next)) => cursor = next,
            _ => break,
        }
        tokio::select! {
            biased;
            _ = &mut stopped => break,
            _ = tokio::time::sleep(interval) => {},
        }
    }
}

#[cfg(unix)]
pub struct Shutdown {
    terminate: tokio::signal::unix::Signal,
    interrupt: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl Shutdown {
    pub fn new() -> std::io::Result<Self> {
        use tokio::signal::unix::{SignalKind, signal};
        Ok(Self {
            terminate: signal(SignalKind::terminate())?,
            interrupt: signal(SignalKind::interrupt())?,
        })
    }

    pub async fn wait(&mut self) {
        tokio::select! {
            _ = self.terminate.recv() => {},
            _ = self.interrupt.recv() => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[tokio::test]
    async fn stopped_or_disconnected_worker_never_starts_a_pass() {
        for send in [true, false] {
            let (stop, stopped) = tokio::sync::oneshot::channel();
            if send {
                stop.send(()).unwrap();
            } else {
                drop(stop);
            }
            run_worker(stopped, std::time::Duration::ZERO, |_| {
                panic!("pass admitted after stop")
            })
            .await;
        }
    }

    #[tokio::test]
    async fn stop_drains_admitted_blocking_pass_without_starting_another() {
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let (started, running) = tokio::sync::oneshot::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let mut admission = Some((started, gate));
        let driver = tokio::spawn(async move {
            run_worker(stopped, std::time::Duration::ZERO, move |_| {
                let (started, gate) = admission.take().expect("second pass admitted");
                let count = count.clone();
                tokio::task::spawn_blocking(move || {
                    count.fetch_add(1, Ordering::SeqCst);
                    started.send(()).unwrap();
                    gate.recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                    Ok(1)
                })
            })
            .await;
        });
        running.await.unwrap();
        stop.send(()).unwrap();
        tokio::task::yield_now().await;
        assert!(
            !driver.is_finished(),
            "admitted blocking operation was abandoned"
        );
        release.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), driver)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(observed.load(Ordering::SeqCst), 1);
    }
}

#[cfg(not(unix))]
pub struct Shutdown;

#[cfg(not(unix))]
impl Shutdown {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self)
    }
    pub async fn wait(&mut self) {
        let _ = tokio::signal::ctrl_c().await;
    }
}
