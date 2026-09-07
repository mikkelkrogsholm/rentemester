const ALLOWED_PRODUCTION_LICENSES = new Set([
  "0BSD",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "MIT",
  "Unlicense",
]);

type LicensePackage = {
  name?: unknown;
  versions?: unknown;
  license?: unknown;
};

export type LicenseReport = Record<string, LicensePackage[]>;

export function verifyProductionLicenses(report: unknown): {
  licenses: string[];
  packages: number;
} {
  if (!report || typeof report !== "object" || Array.isArray(report)) {
    throw new Error("production license report must be an object");
  }

  let packages = 0;
  const licenses = Object.keys(report as LicenseReport).sort();
  for (const license of licenses) {
    if (!ALLOWED_PRODUCTION_LICENSES.has(license)) {
      throw new Error(`production dependency uses an unapproved license: ${license}`);
    }
    const entries = (report as LicenseReport)[license];
    if (!Array.isArray(entries) || entries.length === 0) {
      throw new Error(`production license group ${license} must contain packages`);
    }
    for (const entry of entries) {
      if (
        !entry ||
        typeof entry.name !== "string" ||
        !Array.isArray(entry.versions) ||
        entry.versions.length === 0 ||
        entry.versions.some((version) => typeof version !== "string") ||
        entry.license !== license
      ) {
        throw new Error(`production license group ${license} contains invalid package metadata`);
      }
      packages += 1;
    }
  }
  if (packages === 0) throw new Error("production license report contains no packages");
  return { licenses, packages };
}

if (import.meta.main) {
  const result = Bun.spawnSync([process.execPath, "pm", "licenses", "--prod", "--json"], {
    stdout: "pipe",
    stderr: "pipe",
  });
  if (result.exitCode !== 0) {
    throw new Error(`bun pm licenses failed: ${result.stderr.toString().trim()}`);
  }
  const verified = verifyProductionLicenses(JSON.parse(result.stdout.toString()));
  console.log(
    `production licenses verified: ${verified.packages} packages across ${verified.licenses.join(", ")}`,
  );
}
