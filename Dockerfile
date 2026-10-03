# syntax=docker/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e
ARG BUN_IMAGE=oven/bun:1.4.0-slim@sha256:e0ee68d16ccb9927bf02aa7dd8fd4bf3369ee6d46da04faa72b05ce8bfd135f6
ARG RENTEMESTER_BUN_VERSION=1.4.0
ARG RENTEMESTER_BASE_IMAGE_DIGEST=sha256:e0ee68d16ccb9927bf02aa7dd8fd4bf3369ee6d46da04faa72b05ce8bfd135f6
ARG RENTEMESTER_VERSION
ARG RENTEMESTER_GIT_COMMIT
ARG RENTEMESTER_BUILT_AT
ARG SOURCE_DATE_EPOCH

FROM ${BUN_IMAGE} AS cockpit-build
ARG RENTEMESTER_BUILT_AT
WORKDIR /build
COPY package.json bun.lock bunfig.toml ./
COPY app/package.json app/bunfig.toml ./app/
RUN bun install --frozen-lockfile
COPY app/ ./app/
# The cockpit checks generated tokens and shares the browser-safe permission policy.
COPY DESIGN.md ./
COPY scripts/design-tokens.ts ./scripts/design-tokens.ts
COPY src/design ./src/design
COPY src/core/access-permissions.ts ./src/core/access-permissions.ts
RUN bun run cockpit:build \
    && find app/dist -print0 | xargs -0 touch -d "${RENTEMESTER_BUILT_AT}"

FROM ${BUN_IMAGE} AS runtime-dependencies
ARG RENTEMESTER_BUILT_AT
WORKDIR /build/runtime
COPY package.json bun.lock bunfig.toml ./
COPY app/package.json ./app/package.json
# pdfjs-dist is a locked production runtime dependency; keep its assets in the image.
RUN bun install --frozen-lockfile --production --filter rentemester \
    && find node_modules -print0 | xargs -0 touch -d "${RENTEMESTER_BUILT_AT}"

FROM ${BUN_IMAGE} AS runtime-files
ARG RENTEMESTER_BUILT_AT
WORKDIR /prepared
COPY package.json bun.lock tsconfig.json ./
COPY src ./src
COPY examples ./examples
COPY rules ./rules
COPY sources ./sources
COPY docs ./docs
RUN find /prepared -print0 | xargs -0 touch -d "${RENTEMESTER_BUILT_AT}"

FROM ${BUN_IMAGE} AS image-files
ARG RENTEMESTER_BUILT_AT
WORKDIR /assembled
RUN --mount=type=bind,from=runtime-dependencies,source=/build/runtime/node_modules,target=/mnt/node_modules,readonly \
    --mount=type=bind,from=runtime-files,source=/prepared,target=/mnt/runtime,readonly \
    --mount=type=bind,from=cockpit-build,source=/build/app/dist,target=/mnt/cockpit,readonly \
    mkdir -p ./app/app/dist ./workspace ./import \
    && cp -R /mnt/node_modules ./app/node_modules \
    && cp -R /mnt/runtime/. ./app/ \
    && cp -R /mnt/cockpit/. ./app/app/dist/ \
    && chown -R bun:bun /assembled \
    && find /assembled -print0 | xargs -0 touch -d "${RENTEMESTER_BUILT_AT}"

FROM ${BUN_IMAGE} AS runtime

ARG RENTEMESTER_VERSION
ARG RENTEMESTER_GIT_COMMIT
ARG RENTEMESTER_BUILT_AT
ARG RENTEMESTER_BUN_VERSION
ARG RENTEMESTER_BASE_IMAGE_DIGEST

LABEL org.opencontainers.image.title="Rentemester" \
      org.opencontainers.image.description="Versioneret dansk bogføringsruntime med cockpit" \
      org.opencontainers.image.source="https://github.com/mikkelkrogsholm/rentemester" \
      org.opencontainers.image.version="${RENTEMESTER_VERSION}" \
      org.opencontainers.image.revision="${RENTEMESTER_GIT_COMMIT}" \
      org.opencontainers.image.created="${RENTEMESTER_BUILT_AT}" \
      org.opencontainers.image.base.digest="${RENTEMESTER_BASE_IMAGE_DIGEST}" \
      org.rentemester.runtime.bun.version="${RENTEMESTER_BUN_VERSION}"

# The normalized application tree is copied as one layer before WORKDIR
# exists. Runtime volumes create /workspace and /import as mount points.
COPY --link --from=image-files --chown=1000:1000 /assembled /
WORKDIR /app

ENV RENTEMESTER_WORKSPACE=/workspace \
    RENTEMESTER_DEPLOYMENT_PROFILE=local-container \
    RENTEMESTER_APP_HOST=0.0.0.0 \
    RENTEMESTER_APP_PORT=4319 \
    RENTEMESTER_APP_AUTH=required \
    RENTEMESTER_VERSION=${RENTEMESTER_VERSION} \
    RENTEMESTER_GIT_COMMIT=${RENTEMESTER_GIT_COMMIT} \
    RENTEMESTER_BUILT_AT=${RENTEMESTER_BUILT_AT} \
    RENTEMESTER_BUN_VERSION=${RENTEMESTER_BUN_VERSION} \
    RENTEMESTER_BASE_IMAGE_DIGEST=${RENTEMESTER_BASE_IMAGE_DIGEST}

VOLUME ["/workspace", "/import"]
EXPOSE 4319
USER bun

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["bun", "-e", "const token=process.env.RENTEMESTER_APP_TOKEN; const headers=token?{Authorization:`Bearer ${token}`}:{ }; const r=await fetch('http://127.0.0.1:4319/api/health',{headers}); if(!r.ok) process.exit(1)"]

ENTRYPOINT ["bun", "run", "src/cli.ts"]
CMD ["serve", "--workspace", "/workspace", "--host", "0.0.0.0", "--port", "4319"]
