#!/bin/sh
# Test stand-in for a client binary CodePet must never run during Connect (Codex, OpenCode,
# Grok: their versions come from install paths). Any call is logged and fails loudly.
printf '%s\n' "$0 $*" >> "$HOME/unexpected-calls.log"
exit 99
