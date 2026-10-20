//! Exact two-stage offline operator confirmation; no automatic approval/default.
use serde_json::Value;
use std::io::{BufRead, Write};
pub fn confirm(
    stage: &str,
    review: &Value,
    hash: &str,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    writeln!(
        output,
        "{}",
        serde_json::json!({"stage":stage,"review":review,"reviewHash":hash})
    )?;
    writeln!(
        output,
        "Type the exact reviewHash to continue. Source retirement is persistent; cancelling publication leaves the source retired."
    )?;
    output.flush()?;
    let mut line = String::new();
    std::io::Read::take(input, 67).read_line(&mut line)?;
    let submitted = line.strip_suffix('\n').unwrap_or(&line);
    let submitted = submitted.strip_suffix('\r').unwrap_or(submitted);
    if hash.len() != 64 || submitted != hash {
        return Err(
            "adoption confirmation refused; no implicit retry or source reactivation".into(),
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_exact_review_and_refuses_eof_defaults_and_overlong_input() {
        let hash = "a".repeat(64);
        for line in [
            String::new(),
            "yes\n".into(),
            format!(" {hash}\n"),
            "a".repeat(100),
        ] {
            assert!(
                confirm(
                    "retire-source",
                    &serde_json::json!({"companies":2}),
                    &hash,
                    &mut line.as_bytes(),
                    &mut Vec::new()
                )
                .is_err()
            );
        }
        let mut output = Vec::new();
        assert!(
            confirm(
                "publish-target",
                &serde_json::json!({"companies":2}),
                &hash,
                &mut format!("{hash}\n").as_bytes(),
                &mut output
            )
            .is_ok()
        );
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("leaves the source retired"));
        assert!(output.contains(&hash));
    }
}
