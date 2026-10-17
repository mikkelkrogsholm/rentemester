//! Development process probe, not a production writer/runtime entrypoint.
use rentemester_runtime::WorkspaceLease;
use serde_json::json;
use std::{
    io::{self, BufRead, Write},
    path::Path,
    process::ExitCode,
};
fn emit(value: serde_json::Value) {
    println!("{value}");
    io::stdout().flush().unwrap();
}
fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--idle-child"] {
        std::thread::sleep(std::time::Duration::from_secs(5));
        return ExitCode::SUCCESS;
    }
    if args.len() != 1 {
        emit(json!({"ok":false,"code":"invalid_root"}));
        return ExitCode::from(2);
    }
    let mut lease = match WorkspaceLease::acquire(Path::new(&args[0])) {
        Ok(lease) => lease,
        Err(error) => {
            emit(json!({"ok":false,"code":error.code()}));
            return ExitCode::from(1);
        }
    };
    let id = match lease.ensure_current_root() {
        Ok(id) => id,
        Err(error) => {
            emit(json!({"ok":false,"code":error.code()}));
            return ExitCode::from(1);
        }
    };
    struct ReapChild(Option<std::process::Child>);
    impl Drop for ReapChild {
        fn drop(&mut self) {
            if let Some(child) = &mut self.0 {
                assert!(child.wait().expect("reap development exec probe").success());
            }
        }
    }
    let mut child = ReapChild(None);
    emit(
        json!({"ok":true,"state":"held","scope":"cooperative_process_lease_only","writerAuthority":false,"filesystem":lease.filesystem(),"identity":{"device":id.device.to_string(),"inode":id.inode.to_string()}}),
    );
    for line in io::stdin().lock().lines() {
        match line.as_deref() {
            Ok("spawn-exec-child") if child.0.is_none() => {
                let spawned = std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("--idle-child")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                match spawned {
                    Ok(process) => {
                        emit(json!({"ok":true,"state":"child","pid":process.id()}));
                        child.0 = Some(process);
                    }
                    Err(_) => {
                        emit(json!({"ok":false,"code":"ownership_io"}));
                        return ExitCode::from(1);
                    }
                }
            }
            Ok("release") => {
                drop(lease);
                emit(json!({"ok":true,"state":"released"}));
                return ExitCode::SUCCESS;
            }
            Ok("check") => match lease.ensure_current_root() {
                Ok(_) => emit(json!({"ok":true,"state":"current"})),
                Err(error) => {
                    emit(json!({"ok":false,"code":error.code()}));
                    return ExitCode::from(1);
                }
            },
            _ => {
                emit(json!({"ok":false,"code":"invalid_input"}));
                return ExitCode::from(2);
            }
        }
    }
    ExitCode::SUCCESS
}
