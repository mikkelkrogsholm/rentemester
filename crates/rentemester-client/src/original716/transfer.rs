//! Explicit one-shot native server transport. No shell, build tool, default binary or retry.
use crate::Error;
use std::{
    path::Path,
    process::{Command, Output, Stdio},
};
pub fn execute(server_bin: &Path, args: &[String]) -> Result<Output, Error> {
    if !server_bin.is_absolute() {
        return Err(Error::InvalidInput("--server-bin must be absolute"));
    }
    Command::new(server_bin)
        .arg("--original716-transfer")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| Error::OutcomeUnknown)
}
