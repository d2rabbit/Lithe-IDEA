# Windows application

Windows is a React and Tauri application under [`tauri`](tauri/). It shares
deterministic product behavior with macOS through `rust/lithe-core`; it does
not import Swift code or maintain a second implementation of shared commands.

The same Tauri host also builds on Linux (WebKitGTK). See
[Linux support](#linux-support) below for the current status and
prerequisites.

```text
React features and stores
        |
        v
src/platform/tauri-core.ts
        |
        +-- Tauri platform commands: terminal, watcher, credentials
        |
        `-- platform_invoke/core_execute -> lithe-core
```

The React workbench owns Windows presentation and UI state. Shared search,
Git, history, language, run-configuration, and file behavior belongs in
`lithe-core`. Native terminal, file-watcher, credential, dialog, WebView2,
process, and installer behavior belongs in `windows/tauri/src-tauri` or a
Tauri plugin.

## Development

Required tools are Bun 1.3.x, Rust, and the Windows WebView2/Tauri toolchain.

```powershell
cd windows/tauri
bun install --frozen-lockfile
bun run typecheck
bun run desktop:dev
```

Build the Windows executable through the repository script:

```powershell
./scripts/build-windows.ps1 -Configuration Release
```

The macOS host can run frontend type/build checks and Rust checks, but the
packaged application, WebView2, ConPTY, installer, signing, and full UI flows
must be verified on Windows.

## Migration boundary

Frontend modules import `@/platform/tauri-core`, not
`@tauri-apps/api/core` directly. The platform module keeps native commands
explicit and routes shared operations through one Rust dispatcher. New shared
behavior must add or update the contract and fixtures before both products
consume it.

## Linux support

The React/Tauri host is cross-platform, so Linux shares the Windows
implementation instead of forking a third product. Current status:

- `cargo build` compiles the host unchanged on Linux; Windows-only
  dependencies (`windows-sys`, `common-controls-v6`, the Windows credential
  manager) are target-gated in `tauri/src-tauri/Cargo.toml`, and Linux
  credentials use the Secret Service via `keyring`.
- The frontend is already platform-aware (`IS_LINUX` in
  `tauri/src/utils/platform.ts`); terminal ConPTY options apply to Windows
  only, and Linux terminals use the Unix PTY path of `portable-pty`.
- `tauri.linux.conf.json` adds AppImage/deb/rpm bundle targets, PNG icons, and
  the main window definition. Platform config files only merge on their own
  platform: without the window entry in the Linux config the app would start
  as an invisible background process, because `tauri.windows.conf.json` does
  not apply. Verified by running the app on a GNOME Wayland desktop — the
  window renders out of the box (no `WEBKIT_DISABLE_DMABUF_RENDERER` override
  needed); build with `scripts/build-linux.sh [deb|appimage|rpm]` (defaults to
  deb).
- Font enumeration uses `fc-list` with a built-in fallback list.
- Not yet supported on Linux: the updater pipeline (Windows-only manifests),
  Windows-installer-specific checks, and JIT-backed WebView2 diagnostics.
  Runtime verification requires a Linux desktop with WebKitGTK 4.1.
