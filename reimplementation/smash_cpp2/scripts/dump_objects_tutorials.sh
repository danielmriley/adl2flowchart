#!/usr/bin/env bash
set -euo pipefail
exec "$(dirname "$0")/compare_stdout.sh" objects
