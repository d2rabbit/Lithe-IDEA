#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if find windows \
  \( -path '*/node_modules' -o -path '*/target' -o -path '*/dist' \) -prune -o \
  -type f \( -name '*.cpp' -o -name '*.h' \) -print -quit | grep -q .; then
  echo "Windows product must not restore the retired Qt/C++ implementation." >&2
  exit 1
fi

direct_imports=$(grep -rl 'from "@tauri-apps/api/core"' windows/tauri/src \
  --include='*.ts' --include='*.tsx' | \
  grep -vE '/(core/lithe-core-client|platform/tauri-core)\.ts$' || true)
if [[ -n "$direct_imports" ]]; then
  echo "Frontend modules must use @/platform/tauri-core:" >&2
  echo "$direct_imports" >&2
  exit 1
fi

if grep -q 'src/mocks/tauri-api-mock' windows/tauri/vite.config.ts; then
  echo "Desktop builds must not alias Tauri APIs to browser mocks." >&2
  exit 1
fi

grep -qF 'lithe-core = { path = "../../../rust/lithe-core" }' \
  windows/tauri/src-tauri/Cargo.toml
grep -qF 'platform::platform_invoke' windows/tauri/src-tauri/src/main.rs
grep -qF 'core::core_execute' windows/tauri/src-tauri/src/main.rs
grep -qF 'capabilityForCommand(command)' windows/tauri/src/platform/tauri-core.ts

echo "Windows React/Tauri boundaries verified."
