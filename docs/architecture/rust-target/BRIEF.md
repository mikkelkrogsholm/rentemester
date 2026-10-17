# Preserved user brief — 2026-10-17

The user asked for these deployment possibilities (original Danish):

> Kunne man lave det sådan, at man kunne have en rentemester server og et rentemester web UI, hvis man nu ville køre det på en server? Og man kunne have en desktop app, som man enten kunne køre med serveren som backend, altså man ligesom forbinder til serveren, eller man kører det lokalt

The user then asked whether Rentemester could be rewritten in Rust and said:

> Og det gør ikke noget, at det tager lidt tid at gøre

The assistant proposed a shared Rust backend/CLI/MCP, a GPUI desktop and retained React web UI, with a staged migration. These are assistant proposals, not independently accepted detailed requirements.

The current user request is:

> Målarkitektur
>
> https://github.com/mikkelkrogsholm/gdd

Mandate: produce a concrete target architecture through GDD. Planning, local artifacts and their verification are authorized. This request does not authorize implementing the rewrite, migrating live data, deployment or unattended work. Existing product rules, company isolation and data confidentiality remain binding. Time tolerance is not a reason to add unnecessary components.
