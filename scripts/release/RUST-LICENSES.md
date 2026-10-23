# Locked Rust and PDFium dependency policy — R07 / #679

The Bun production license policy stays in `check-production-licenses.ts`.
`rust-license-policy.json` applies to the existing Rust dependency graph;
`pdfium-license-approval.json` approves explicit PDFium artifact contents.
Neither changes Rentemester's own license.

Run `bun run supply-chain:licenses:rust` to resolve complete locked all-platform
Cargo metadata with `--all-features` and check it. `--metadata FILE` on `verify-rust-licenses.py` reuses
an existing complete capture. This does not compile Rust. Partial/no-dependency
metadata, unknown third-party licenses, lock/source/version/checksum drift and
malformed expressions refuse. Workspace membership and an owned manifest identify
our eleven product crates; patched `vendor/` crates remain third-party code.

SPDX `OR` may choose an approved branch. `AND` retains every obligation. `WITH`
is a separate expression requiring explicit approval; accepting Apache-2.0 does
not automatically accept any exception. Historical slash metadata is normalized
only for exact named releases and original expressions in the policy. Most such
entries conservatively retain both permissive grants; the two Unlicense/MIT
entries select their actual bundled MIT grant. Another version/expression needs
a new review. Nothing globally interprets arbitrary slashes as permission.

The current full lock contains 1,043 packages. Default-feature metadata exposes
560; the remaining 483 include the retained optional `legacy-gpui` graph. The
checker refuses that incomplete capture and audits all locked feature graphs.
This preserves historical source without selecting GPUI as the product desktop;
the agreed product remains Tauri with the shared React UI. No Cargo dependency
or feature is added by this license change.

The engineering decisions for dependencies already consumed by this rewrite are:

| Terms | Scoped decision and retained obligations |
| --- | --- |
| Original six Bun-approved identifiers | Reuse those identifiers for Rust; retain copyright, license and applicable NOTICE text. |
| Unicode-3.0 | Add the Unicode permission notice for the existing Unicode/ICU dependencies; retain it with copies or documentation. [Unicode text](https://www.unicode.org/license.txt). |
| Zlib | Add the existing compression/hash dependencies' permissive terms; retain notice and identify altered source without claiming upstream authorship. [Zlib text](https://spdx.org/licenses/Zlib.html). |
| CDLA-Permissive-2.0 | Existing root-certificate data may be shared with its agreement text retained. This does not infer a general policy for other data licenses. [CDLA text](https://cdla.dev/permissive-2-0/). |
| 0BSD, CC0-1.0, NCSA | Existing optional-feature dependencies retain their grants and disclaimers; NCSA keeps binary/source notices and no-endorsement conditions. [0BSD](https://spdx.org/licenses/0BSD.html), [CC0](https://creativecommons.org/publicdomain/zero/1.0/legalcode.en), [NCSA](https://spdx.org/licenses/NCSA.html). |
| MPL-2.0 | Exact current versions of cssparser, cssparser-macros, dtoa-short, option-ext, selectors and the retained optional-feature dependencies cbindgen/dwrote only. Preserve notices and version-pinned upstream source availability; modifications to covered files require renewed review and source availability. No blanket approval of other MPL dependencies or upgrades. [License](https://www.mozilla.org/en-US/MPL/2.0/), [upstream FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/). |
| Apache-2.0 WITH LLVM-exception | Exact target-lexicon release only; retain its actual Apache/exception text. Other packages or exception expressions are not implicitly approved. [Exception text](https://spdx.org/licenses/LLVM-exception.html). |

Each scoped exception records its version, exact original expression, rationale
and upstream source URL. Cargo.lock checksums bind registry source; locally
patched dependencies remain separately identified. This is an engineering
dependency decision and verification policy, not an assertion that a release has
already satisfied every distribution obligation.

PDFium is outside Cargo's graph. All four archives in
`scripts/rust/pdf-worker/artifacts.json` have a source-reviewed inventory of the
complete root `LICENSE` and every nested `licenses/` file. Whole archive, library
and notice hashes identify the precise release and contents. The inventory alone
does not approve terms: the main policy separately approves its exact hash after
review. Neither a filename nor a nonempty directory counts as license approval.

The existing Rust candidate identity builder and promotion verifier compare the
complete selected artifact's notice paths, sizes and content hashes against that
reviewed inventory. Missing, additional or changed notices refuse; a changed
inventory or PDFium pin also needs an explicit new policy decision. This retains
the full bundled notices, including compound third-party notices, rather than
replacing them with guessed SPDX labels.

CI checks the locked Rust graph in addition to the existing Bun license check.
Targeted synthetic tests exercise policy/expression/lock and PDFium content
refusals. Actual dependency/archive receipts supplement those tests. Existing
foundation/fresh-build and source boundary receipts are reused where source
remains applicable. No financial/provider effects or unchanged Rust builds are
replayed for this policy change. Packaging, platform qualification, release
approval, cutover and the final installed desktop acceptance keep their original
scope and remain separate gates.
