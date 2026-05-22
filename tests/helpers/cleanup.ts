import { rmSync } from "node:fs";

/**
 * Platform-safe recursive directory removal. On Windows, SQLite file handles
 * may not be fully released by the time cleanup runs, causing EBUSY errors.
 * This helper retries once after a GC nudge (Bun.gc) before giving up.
 *
 * Use in `afterAll` / `finally` blocks instead of bare `rmSync`.
 */
export function cleanupDir(...dirs: string[]): void {
  for (const dir of dirs) {
    try {
      rmSync(dir, { recursive: true, force: true });
    } catch (err: unknown) {
      if (
        typeof err === "object" &&
        err !== null &&
        "code" in err &&
        (err as { code: string }).code === "EBUSY" &&
        process.platform === "win32"
      ) {
        Bun.gc(true);
        try {
          rmSync(dir, { recursive: true, force: true });
        } catch {
          // Best-effort: the OS will clean temp dirs eventually.
        }
      }
    }
  }
}
