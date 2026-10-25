//! Explicit desktop ownership channel; standalone servers never read stdin here.
use tokio::sync::oneshot;

pub fn validate_flags(
    enabled: bool,
    local_runtime: bool,
    credential_stdin: bool,
) -> Result<(), &'static str> {
    if enabled && (!local_runtime || credential_stdin) {
        return Err("--managed-parent-stdin requires --local-runtime-dir and exclusive stdin");
    }
    Ok(())
}

pub fn monitor(enabled: bool) -> std::io::Result<Option<oneshot::Receiver<()>>> {
    if !enabled {
        return Ok(None);
    }
    let (stop, stopped) = oneshot::channel();
    // Not spawn_blocking: an idle live owner's pipe must not hold Tokio's
    // blocking pool/runtime open after SIGTERM. Process exit ends this thread.
    std::thread::Builder::new()
        .name("managed-parent".into())
        .spawn(move || {
            watch(std::io::stdin().lock());
            let _ = stop.send(());
        })?;
    Ok(Some(stopped))
}

fn watch(mut input: impl std::io::Read) {
    let mut buffer = [0; 64];
    loop {
        match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

pub async fn lost(stopped: Option<oneshot::Receiver<()>>) {
    match stopped {
        Some(stopped) => {
            let _ = stopped.await;
        }
        None => std::future::pending::<()>().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_option_is_local_only_and_never_shares_credential_stdin() {
        assert!(validate_flags(true, true, false).is_ok());
        assert!(validate_flags(true, false, false).is_err());
        assert!(validate_flags(true, true, true).is_err());
        assert!(validate_flags(false, false, true).is_ok());
    }

    #[tokio::test]
    async fn shutdown_parent_eof_not_data_and_disabled_never_signals() {
        assert!(monitor(false).unwrap().is_none());
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), lost(None))
                .await
                .is_err()
        );
        let (reader, mut writer) = std::os::unix::net::UnixStream::pair().unwrap();
        let (stop, mut stopped) = oneshot::channel();
        let thread = std::thread::spawn(move || {
            watch(reader);
            let _ = stop.send(());
        });
        std::io::Write::write_all(&mut writer, b"still owned").unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut stopped)
                .await
                .is_err()
        );
        drop(writer);
        tokio::time::timeout(std::time::Duration::from_secs(2), stopped)
            .await
            .unwrap()
            .unwrap();
        thread.join().unwrap();
    }
}
