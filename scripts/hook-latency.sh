#!/usr/bin/env bash
# Isolated WP7 latency measurements. No clients, installation or real pet state.
# Usage: scripts/hook-latency.sh --bin /path/to/release/codepet --load python --out DIR
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
exec python3 -I "$here/hook_latency.py" "$@"
