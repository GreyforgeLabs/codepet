#!/usr/bin/env bash
# Run from the outer session; the Python controller verifies the supplied nested target.
set -euo pipefail
exec python3 "$(dirname -- "$(readlink -f -- "$0")")/rig_qa.py" "$@"
