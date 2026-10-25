//! Process-local launcher constraints, not a privilege or policy grant.
/// Tightening this child process's umask does not change user/host settings.
#[cfg(unix)]
pub fn private_process() -> u32 {
    rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o077));
    rustix::process::getuid().as_raw()
}
