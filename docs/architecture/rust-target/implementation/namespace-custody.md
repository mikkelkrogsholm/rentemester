# Physical namespace custody in native control schema 32

The Rust server binds an admitted workspace to the device/inode identity of the
root held by its existing exclusive namespace lease. Normal startup and dispatch
verify that binding. A raw directory copy has a different physical identity and
is refused; ordinary startup never enrolls, migrates or repairs a binding.

## Existing native schema 31

The operator must first select the sole authoritative source, stop/drain its
server and jobs, and account for other copies and old software under the existing
cutover procedure. With the existing private server configuration and its explicit
operator actor/allowlist, run:

```sh
rentemester-server --enroll-namespace-custody \
  --config /absolute/private/server.json \
  --confirm-target /absolute/exact/configured/workspace
```

This exclusive offline action atomically validates complete schema 31, appends
native migration 32 and binds the held root. Existing migration provenance,
sessions, business rows and audit evidence remain unchanged. A live lease,
different confirmation, denied operator, frozen/moved/staged state, mismatched
binding or missing binding in an already-current schema is refused. Investigate a
refusal using the retained source and configuration; do not edit SQL, remove the
binding, or retry against another copy to make it open.

Fresh creation and trusted signed restore/staging/adoption bind their newly held
physical root before ordinary admission. Publication preserves the stage inode.
Portable business archives remain control context 30/company schema 36 and do not
export or import physical custody. Restore to a fresh directory through the
verified recovery workflow instead of relocating or copying an enrolled root.
The separate explicit mail-schema action remains 30→31, or validation-only on an
already-current schema; it does not silently enroll custody.

## Compatibility and cutover limits

A supported older runtime with the complete-schema guard refuses physical schema
32. Previously copied schema-31 data plus a previously copied schema-31 binary
cannot be retrospectively revoked by this local binding. The original sole-source
selection, stop/drain, custody and retirement gate therefore remains mandatory;
this change is not a global lease service.

Prior retained schema-31 desktop/server packages remain evidence for their recorded
revisions, not qualification of the new schema-32 candidate. Rebuild and qualify a
current package before distribution. Final installed desktop and release/platform
acceptance remains outstanding; this correction alone does not close #726.
