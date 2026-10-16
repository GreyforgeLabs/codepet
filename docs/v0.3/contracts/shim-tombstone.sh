#!/bin/sh
# managed by codepet: tombstone
exec >/dev/null 2>&1
timeout -s KILL 0.3 cat >/dev/null 2>&1; exit 0
