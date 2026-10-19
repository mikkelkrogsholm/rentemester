import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

/** Actual password + one-time MFA login; secrets stay in private stdin/files. */
export async function checkNativeCliLogin(base: string, code: string) {
  const binary = process.env.RM_NATIVE_CLI_BIN;
  if (!binary) return;
  const directory = mkdtempSync(join(tmpdir(), "rentemester-cli-login-"));
  try {
    const file = join(directory, "session.cookie");
    const prefix = ["remote", "--server", base, "--allow-loopback-http"];
    const args = [...prefix, "login", "--email", "owner@example.invalid", "--factor", "backup", "--session-out", file];
    const output = execFileSync(binary, args, { input: `synthetic-password-123\n${code}\n`, encoding: "utf8", timeout: 30000 });
    if (JSON.parse(output).data?.status !== "authenticated") throw new Error("CLI password/MFA login did not authenticate");
    const cookie = readFileSync(file, "utf8");
    if ((statSync(file).mode & 0o777) !== 0o600 || output.includes(code) || output.includes(cookie)) throw new Error("CLI login exposed secrets or created nonprivate session file");
    const me = JSON.parse(execFileSync(binary, [...prefix, "--cookie-file", file, "me"], { encoding: "utf8", timeout: 30000 }));
    if (!me.ok || !me.data?.ok || !JSON.stringify(me.data).includes("owner@example.invalid")) throw new Error("New CLI MFA session cannot read current synthetic owner");
    const before = readFileSync(file, "utf8");
    const duplicate = spawnSync(binary, args, { input: "", encoding: "utf8", timeout: 10000 });
    if (duplicate.status !== 2 || readFileSync(file, "utf8") !== before) throw new Error("CLI login overwrote existing private session output");
    console.log("PASS: real CLI password → one-time MFA → private session file → authenticated owner; existing session file protected");
  } finally { rmSync(directory, { recursive: true, force: true }); }
}
