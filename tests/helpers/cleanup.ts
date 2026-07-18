/**
 * Cross-platform temp-directory cleanup helper (#323).
 *
 * On Windows, SQLite file handles may not be fully released immediately
 * after `db.close()`. This causes `EBUSY` errors when `rmSync()` tries
 * to delete the directory. The helper retries once after a forced GC
 * cycle on win32; on Unix it is a thin passthrough.
 */

import { rmSync } from "node:fs";

const IS_WINDOWS = process.platform === "win32";
const MAX_RETRIES = 3;
const RETRY_DELAY_MS = 50;

function sleepSync(ms: number): void {
  if (ms <= 0) return;
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

/**
 * Removes a directory tree, retrying on EBUSY (Windows SQLite handle leak).
 * Safe to call on all platforms — on Unix the first attempt always succeeds.
 */
export function cleanupTempDir(dir: string): void {
  for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
    try {
      rmSync(dir, { recursive: true, force: true });
      return;
    } catch (err: unknown) {
      const code = (err as NodeJS.ErrnoException).code;
      if ((code === "EBUSY" || code === "EPERM") && IS_WINDOWS && attempt < MAX_RETRIES) {
        // Force GC to release SQLite handles, then retry
        if (typeof Bun !== "undefined" && typeof Bun.gc === "function") {
          Bun.gc(true);
        }
        sleepSync(RETRY_DELAY_MS);
        continue;
      }
      throw err;
    }
  }
}
