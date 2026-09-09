# Lithe Agent Entry Point

Before any work in this repository, load and follow the `develop-lithe` skill
at `.agents/skills/develop-lithe/SKILL.md`. That skill is the single source of
truth for AI coding and verification rules, including the required Rust Core
comment standard.

If a task creates, modifies, or reviews test code or test infrastructure,
additionally load `.agents/skills/write-stable-tests/SKILL.md` before
proceeding. That skill defines the mandatory bounded-wait, deterministic-time,
cleanup, and per-test timing rules for both macOS and Windows.

If a task prepares, validates, or publishes a stable Lithe release,
additionally load `.agents/skills/release-lithe/SKILL.md` before changing
release notes, version metadata, tags, or release workflows.

If the task involves building, running, diagnosing, or transferring files to the
Windows product through a Parallels guest VM, additionally load
`.agents/skills/debug-windows-on-parallels/SKILL.md` before proceeding.

## Repository Overview

Lithe is a low-memory IntelliJ IDEA alternative for Java and Spring Boot
development, built as two independent platform products over a shared Rust
core:

- `macos/` — reference product. SwiftUI/AppKit app under
  `macos/Sources/Lithe/` plus independently owned `Lithe<Feature>Module`
  targets. The SwiftPM package (`Package.swift`) sits at the repo root;
  Swift 6.2 toolchain.
- `rust/lithe-core/` — shared deterministic commands, models, validation, JSON
  envelope, and C ABI consumed by both platforms. `lithe-db-mcp` and
  `lithe-db-sidecar` are database helpers.
- `windows/tauri/` — independent React + Tauri 2 product (Bun for frontend
  scripts). Must not import Swift source or depend on macOS types.
- `shared/` — cross-platform JSON contracts (`shared/contracts/`) and reusable
  fixtures (`shared/fixtures/projects/`); not compiled implementation.
- `Plugins/mac/` and `Plugins/win/` — platform-owned plugin packages; neither
  platform compiles the other's plugin tree.
- `scripts/`, `docs/`, `infra/`, `third_party/` — build/verification tooling,
  documentation, repository-level validation containers, pinned upstream code.

## Common Commands

Run from the repository root:

- Dev build + launch macOS app: `./scripts/preview.sh` (builds and links Rust
  Core first). Swift-only validation: `swift run --disable-sandbox Lithe`.
- macOS tests: `./scripts/test-macos.sh`.
- Rust Core: `./scripts/verify-rust-core-comments.sh` first (fast), then
  `./scripts/verify-rust-core.sh`.
- Boundaries and contracts: `./scripts/verify-service-boundaries.sh`,
  `./scripts/verify-shared-contracts.sh`,
  `./scripts/verify-windows-boundaries.sh` (runnable from macOS/Linux).
- Feature checks: `./scripts/verify-core.sh`,
  `./scripts/verify-git-graph.sh`,
  `./scripts/test-git-performance-baseline.sh`.
- Packaging: `./scripts/package-app.sh` (macOS app bundle),
  `./scripts/build-windows.ps1 -Configuration Release` (Windows; then
  `cargo test --manifest-path windows/tauri/src-tauri/Cargo.toml` on Windows).

Pick the smallest check matching the change; the full change-to-validation
matrix is in the `develop-lithe` skill. Never claim a check passed that you
could not run on the current machine.

## Key Boundary Rules

- macOS layering: Views → Application feature models → Services → Core ports →
  typed Rust adapters. Core and Services must stay free of SwiftUI, AppKit,
  `Process`, and direct platform APIs. `MacServiceContainer` is the composition
  root; platform capabilities live in `macos/Sources/Lithe/Platform/MacOS/`.
- Command names, JSON fields, error codes, C symbols, and module/capability IDs
  are compatibility surfaces — moving or refactoring files must not change
  them.
- Deterministic behavior shared by both products belongs in
  `rust/lithe-core/`; add a fixture under `shared/fixtures/` before a second
  platform relies on new shared behavior.
- Windows feature code imports `@/platform/tauri-core`, not the Tauri core API
  directly. Shared operations route through lithe-core; Windows-only native
  behavior stays in the Tauri host or a platform plugin.
- Never embed developer-machine paths, credentials, or tool installation paths
  in application logic; resolve them through adapters or configuration.

## Key Docs

- `docs/architecture/repository-layout.md` — directory ownership, sharing
  rules, Rust Core package layout, and the full Rust Core comment standard.
- `shared/contracts/application-boundary.md` and
  `shared/contracts/rust-core-api.md` — cross-platform contracts.
- `docs/architecture/language-tooling.md` — LSP protocol/application split
  between Rust and platform services.

## Test Process Lifecycle and Cleanup

Unless the user gives a specific instruction to keep a process running, any
Lithe application started for building, testing, debugging, previewing, or
verification must be shut down when the task or test run is complete. Clean up
all child processes, helper processes, temporary app instances, and related
resources, then verify that no Lithe processes remain before handing the work
back. Do not launch duplicate Lithe instances during repeated checks, and do
not leave test-built applications open in the user's application list. If a
process cannot be stopped cleanly, report it explicitly and make a bounded
best-effort cleanup before continuing.

## High-Performance UI Interaction and Resizable Layout Requirements

When working on draggable splitters, resizable panels, continuous dragging,
scrolling, or other high-frequency UI interactions, prioritize reusing the
project's existing high-performance layout containers and interaction
components. Do not quickly implement these behaviors by stacking custom
`DragGesture` handlers in a business parent view, writing to multiple `@State`
properties on every event, or duplicating splitter logic.

- Resizable macOS panels must use `LitheSplitPaneView` and `SplitHandleView`
  whenever possible. If they genuinely cannot be reused, explain why in the
  change summary and preserve the same behavioral contract.
- Drag handling must use a stable coordinate space, preferably global
  coordinates for continuous dragging, to prevent the moving splitter from
  changing the coordinate origin and causing jumps.
- High-frequency drag events must be throttled, coalesced, or filtered with a
  dead zone. Do not trigger unnecessary parent-view reconstruction on every
  pointer event. Keep mutable size state in a local layout container whenever
  possible so dragging does not recompute the entire feature page.
- Provide explicit minimum and maximum sizes and available-space constraints so
  adjacent panels retain their minimum usable widths. Window resizing, panel
  hiding, and panel restoration must not produce negative sizes or layout
  overflow.
- Interaction behavior should match existing Git, editor, and tool windows,
  including hover/drag highlighting, the platform-appropriate resize cursor,
  help text, and accessibility labels.
- If sizes must persist across refreshes or restarts, commit the final size
  through the existing layout-persistence mechanism rather than continuously
  writing to persistent storage during dragging.
- After adding or modifying this type of UI, at minimum complete the relevant
  product build, `git diff --check`, and boundary checks. During code review,
  explicitly confirm that dragging does not cause high-frequency full-page
  redraws.

These requirements apply to both macOS and Windows. Each platform may use its
own native implementation, but interaction semantics, performance goals, size
constraints, and accessibility requirements must remain consistent.
