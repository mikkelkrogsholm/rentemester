#!/bin/sh
set -eu
umask 077
# No inferred create, upgrade, restore, chown or worker activation on restart.
exec /opt/rentemester/bin/rentemester-server \
    --listen 0.0.0.0:4319 --web-dir /opt/rentemester/web \
    --config "$RENTEMESTER_CONFIG" --origin "$RENTEMESTER_ORIGIN" "$@"
