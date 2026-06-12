#!/usr/bin/env node
//
// verify-rentemester-backup — a self-contained Node.js/Bun script that
// verifies a Rentemester backup .tar's Ed25519 signature against a supplied
// public key. Distributable as one file: an accountant or auditor can run
// this WITHOUT Rentemester installed (only Node.js >= 18, or Bun).
//
// Usage:
//   node verify-rentemester-backup.mjs <backup.tar> <public-key.pem>
//
// Exit codes:
//   0  — signature is valid (the backup's manifest.json was signed by the
//        supplied public key's matching private key).
//   1  — signature does NOT match (tampering, wrong key, or corrupt archive).
//   2  — usage error or missing files.
//
// The script reads the .tar in pure JS (vendored mini-reader, ~80 lines),
// pulls out `manifest.json` and `manifest.json.ed25519.sig`, base64-decodes
// the signature, and verifies it with Node's built-in `crypto.verify`. No
// network, no external dependencies, no exotic imports — exactly what an
// auditor wants when reproducing a verification on a clean machine.

import { createPublicKey, verify as cryptoVerify } from "node:crypto";
import { readFileSync, existsSync } from "node:fs";
import { argv, exit } from "node:process";

const BLOCK = 512;

// --- vendored mini tar reader (matches src/core/tar.ts's readTar) ----------

function parseOctal(field) {
  let text = "";
  for (const byte of field) {
    if (byte === 0 || byte === 0x20) break;
    text += String.fromCharCode(byte);
  }
  if (text.length === 0) return 0;
  const value = parseInt(text, 8);
  if (Number.isNaN(value)) throw new Error("tar: malformed octal field");
  return value;
}

function parseString(field) {
  const nul = field.indexOf(0);
  return field.subarray(0, nul === -1 ? field.length : nul).toString("utf8");
}

function isZeroBlock(block) {
  for (const byte of block) if (byte !== 0) return false;
  return true;
}

function pad512(size) {
  const rem = size % BLOCK;
  return rem === 0 ? 0 : BLOCK - rem;
}

function readTar(archive) {
  if (archive.length % BLOCK !== 0) {
    throw new Error("tar: archive length is not a multiple of 512");
  }
  const entries = [];
  let offset = 0;
  let sawTerminator = false;
  while (offset + BLOCK <= archive.length) {
    const header = archive.subarray(offset, offset + BLOCK);
    if (isZeroBlock(header)) {
      sawTerminator = true;
      break;
    }
    // Verify the header checksum — cheap corruption detection.
    const stored = parseOctal(header.subarray(148, 156));
    let sum = 0;
    for (let i = 0; i < BLOCK; i += 1) {
      sum += i >= 148 && i < 156 ? 0x20 : header[i];
    }
    if (sum !== stored) {
      throw new Error("tar: header checksum mismatch (archive is corrupt or tampered)");
    }
    const name = parseString(header.subarray(0, 100));
    const prefix = parseString(header.subarray(345, 500));
    const fullPath = prefix ? `${prefix}/${name}` : name;
    const size = parseOctal(header.subarray(124, 136));
    const typeflag = header[156];
    offset += BLOCK;
    if (size < 0 || offset + size + pad512(size) > archive.length) {
      throw new Error(`tar: archive is truncated — entry '${fullPath}' body extends past end`);
    }
    const content = archive.subarray(offset, offset + size);
    offset += size + pad512(size);
    if (typeflag === 0x30 || typeflag === 0) {
      entries.push({ path: fullPath, content: Buffer.from(content) });
    }
  }
  if (!sawTerminator) {
    throw new Error("tar: archive is missing its terminating zero block (truncated)");
  }
  return entries;
}

// --- verification driver ---------------------------------------------------

function usage(stream = process.stderr, code = 2) {
  stream.write(
    "Usage: node verify-rentemester-backup.mjs <backup.tar> <public-key.pem>\n",
  );
  exit(code);
}

function main() {
  const [, , tarPath, pubKeyPath, ...rest] = argv;
  if (!tarPath || !pubKeyPath || rest.length > 0) usage();
  if (!existsSync(tarPath)) {
    process.stderr.write(`Backup archive not found: ${tarPath}\n`);
    exit(2);
  }
  if (!existsSync(pubKeyPath)) {
    process.stderr.write(`Public key file not found: ${pubKeyPath}\n`);
    exit(2);
  }

  let entries;
  try {
    entries = readTar(readFileSync(tarPath));
  } catch (err) {
    process.stderr.write(`FAIL: ${err && err.message ? err.message : String(err)}\n`);
    exit(1);
  }

  const manifest = entries.find((e) => e.path === "manifest.json");
  const sigEntry = entries.find((e) => e.path === "manifest.json.ed25519.sig");
  if (!manifest) {
    process.stderr.write("FAIL: backup archive does not contain manifest.json\n");
    exit(1);
  }
  if (!sigEntry) {
    process.stderr.write(
      "FAIL: backup archive does not contain manifest.json.ed25519.sig — was it signed with --sign-with-ed25519?\n",
    );
    exit(1);
  }

  const sigBase64 = sigEntry.content.toString("utf8").trim();
  let signature;
  try {
    signature = Buffer.from(sigBase64, "base64");
  } catch {
    process.stderr.write("FAIL: signature file is not valid base64\n");
    exit(1);
  }

  let publicKey;
  try {
    publicKey = createPublicKey(readFileSync(pubKeyPath, "utf8"));
  } catch (err) {
    process.stderr.write(
      `FAIL: could not parse public key — ${err && err.message ? err.message : String(err)}\n`,
    );
    exit(2);
  }

  // Ed25519 verify uses a null algorithm name (the curve is implied).
  const ok = cryptoVerify(null, manifest.content, publicKey, signature);
  if (!ok) {
    process.stderr.write(
      "FAIL: signature does NOT match — the manifest was not signed by the supplied key (or the archive was tampered).\n",
    );
    exit(1);
  }

  process.stdout.write(
    `OK: manifest.json signature verifies against ${pubKeyPath}.\n`,
  );
  exit(0);
}

main();
