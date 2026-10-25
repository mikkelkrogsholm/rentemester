//! Original workspace snapshot/restore syntax, explicit one-shot common server.
use crate::dated_reports::Output;
use std::collections::BTreeMap;
fn usage(s: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{s}\n"),
        exit: 2,
    }
}
fn accepts(a: &[String]) -> bool {
    a.first().map(String::as_str) == Some("workspace")
        && a.get(1)
            .is_some_and(|s| matches!(s.as_str(), "snapshot" | "restore"))
}
pub fn prepare(args: &[String]) -> Option<Vec<String>> {
    let (mut words, mut i) = (vec![], 0);
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        if k.starts_with("--") {
            i = if a.is_some() || matches!(k, "--json" | "--help") {
                i + 1
            } else {
                crate::original_args::value(args, i).map_or(i + 1, |(_, n)| n)
            };
        } else {
            words.push(i);
            i += 1;
        }
    }
    if words.len() < 2 {
        return None;
    }
    let selected = vec![args[words[0]].clone(), args[words[1]].clone()];
    if !accepts(&selected) {
        return None;
    }
    let mut out = selected;
    out.extend(
        args.iter()
            .enumerate()
            .filter(|(i, _)| !words[..2].contains(i))
            .map(|(_, v)| v.clone()),
    );
    Some(out)
}
pub async fn execute(args: &[String]) -> Option<Output> {
    if !accepts(args) {
        return None;
    }
    Some(run(args))
}
fn run(args: &[String]) -> Output {
    let mut flags = BTreeMap::new();
    let mut i = 2;
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        if !matches!(
            k,
            "--workspace"
                | "--snapshot"
                | "--out"
                | "--target"
                | "--target-workspace"
                | "--confirm"
                | "--at"
                | "--actor"
                | "--json"
                | "--format"
                | "--help"
                | "--config"
                | "--server-bin"
                | "--cookie-file"
                | "--origin"
                | "--trust-keys"
                | "--source-unavailable"
                | "--artifact-format"
                | "--trusted-backup-key"
        ) {
            return usage(&format!("Unknown flag {k}"));
        }
        let (v, n) = if matches!(k, "--json" | "--help") {
            if a.is_some() {
                return usage("Boolean flag takes no value");
            }
            ("true", i + 1)
        } else {
            match crate::original_args::value(args, i) {
                Some(v) => v,
                None => return usage("Missing flag value"),
            }
        };
        if flags.insert(k, v).is_some() {
            return usage("Duplicate flag");
        }
        i = n;
    }
    if flags
        .get("--format")
        .is_some_and(|v| !matches!(*v, "json" | "human"))
    {
        return usage("--format must be json or human");
    }
    let native = match flags
        .get("--artifact-format")
        .copied()
        .unwrap_or("source-tar-v1")
    {
        "source-tar-v1" => false,
        "native-v1" => true,
        _ => return usage("--artifact-format must be source-tar-v1 or native-v1"),
    };
    if native && flags.contains_key("--trust-keys") || !native && flags.contains_key("--trusted-backup-key") {
        return usage("Trust controls must match the explicitly selected artifact profile");
    }
    if native && flags.contains_key("--at") {
        return usage("native-v1 uses the admitted runtime clock; --at belongs to source-tar-v1");
    }
    if flags.contains_key("--help") {
        return Output {
            stdout: format!(
                "workspace snapshot --workspace <dir> --out <tar> --confirm yes [--at <ISO-timestamp>]\nworkspace restore --snapshot <tar> --target-workspace <dir> --confirm yes\n\nRust operator controls: --config <private0600.json> --server-bin <absolute-native-server>\nSnapshot also requires --cookie-file <private0600> --origin <origin>. Restore requires --trust-keys <JSON slug-to-PEM map> --source-unavailable yes; --target is an alias of --target-workspace. --artifact-format source-tar-v1 is the original TAR profile; native-v1 explicitly selects rentemester-native-backup/1 and versioned rentemester-cli-workspace/2 receipts. native-v1 preserves current native business sections and bank originals, requires configured backupSigningKeyFile for snapshot and --trusted-backup-key RAW_PUBLIC_KEY for restore. This is never a TAR or an implicit format fallback. Embedded public keys never grant trust. Source legacy schemas/nonempty unsupported business require explicit versioned migration. No automatic retry.\n"
            ),
            stderr: String::new(),
            exit: 0,
        };
    }
    if flags
        .get("--confirm")
        .is_none_or(|v| !v.trim().eq_ignore_ascii_case("yes"))
    {
        return Output {
            stdout: "{\"ok\":false,\"errors\":[\"--confirm yes required\"]}\n".into(),
            stderr: String::new(),
            exit: 1,
        };
    }
    let required = if args[1] == "snapshot" {
        vec!["--workspace", "--out", "--cookie-file", "--origin"]
    } else {
        if native {
            vec!["--snapshot", "--trusted-backup-key", "--source-unavailable"]
        } else {
            vec!["--snapshot", "--trust-keys", "--source-unavailable"]
        }
    };
    for k in required.into_iter().chain(["--config", "--server-bin"]) {
        if flags.get(k).is_none_or(|v| v.trim().is_empty()) {
            return usage(&format!("Missing required {k}"));
        }
    }
    if args[1] == "restore"
        && !flags.contains_key("--target")
        && !flags.contains_key("--target-workspace")
    {
        return usage("workspace restore requires --snapshot and --target-workspace");
    }
    if flags.contains_key("--target") && flags.contains_key("--target-workspace") {
        return usage("Use only one target flag");
    }
    let bin = std::path::Path::new(flags["--server-bin"]);
    if !bin.is_absolute() {
        return usage("--server-bin must be absolute");
    }
    match rentemester_client::original716::transfer::execute(bin,args){
        Ok(o)=>Output{stdout:String::from_utf8_lossy(&o.stdout).into_owned(),stderr:String::from_utf8_lossy(&o.stderr).into_owned(),exit:u8::try_from(o.status.code().unwrap_or(1)).unwrap_or(1)},
        Err(_)=>Output{stdout:String::new(),stderr:"native transfer outcome unknown; reconcile source keys, snapshot files and private stages before another deliberate attempt (no fallback or retry)\n".into(),exit:1},
    }
}
