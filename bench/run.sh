#!/usr/bin/env bash
# Clone a tiered set of real TypeScript projects and run the latency harness
# against them. Projects land in bench/projects/ (gitignored). Idempotent:
# re-running skips already-cloned repos.
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p projects

clone() { # <url> <dir>
  if [ ! -d "projects/$2" ]; then
    echo "cloning $2…"
    git clone --quiet --depth 1 "$1" "projects/$2"
  fi
}

# small → large, all predominantly hand-written TypeScript.
clone https://github.com/pmndrs/zustand zustand
clone https://github.com/colinhacks/zod zod
clone https://github.com/trpc/trpc trpc
clone https://github.com/Effect-TS/effect effect

# Use the rustup stable toolchain explicitly (a Homebrew rustc may shadow it).
RUSTUP_RUSTC="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc"
[ -x "$RUSTUP_RUSTC" ] && export RUSTC="$RUSTUP_RUSTC"
(rustup run stable cargo build --release -p agent_doctor_bench 2>/dev/null) \
  || cargo build --release -p agent_doctor_bench

echo
../target/release/agent-doctor-bench projects/*/
