#!/usr/bin/env bash
# Run the viewer inside the nix shell that provides the windowing libs.
# Usage: ./run.sh [extra cargo-run args...]
set -euo pipefail
cd "$(dirname "$0")"
exec nix-shell --run "cargo run --release -- $*"
