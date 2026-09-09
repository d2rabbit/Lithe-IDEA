/**
 * Import pipeline for `.vsix` packages.
 *
 * The shared Rust Core parses the archive (`extensions.inspectVsix`) so both
 * platforms agree on the result; this module maps the inspection to Lithe
 * contributions, registers them with the theme/icon-theme/snippet registries,
 * and persists the mapped manifest locally. It intentionally avoids the
 * native `install_extension*` backend commands, which stay reserved for
 * packaged marketplace extensions.
 */

import { invoke } from "@/platform/tauri-core";
import { extensionRegistry } from "../registry/extension-registry";
import {
  activateExtensionContributions,
  deactivateExtensionContributions,
} from "../runtime/extension-contribution-runtime";
import { extensionIdForVsix, mapVsixInspectionToManifest } from "./vsix-contribution-mapper";
import {
  readImportedVsixManifests,
  removeImportedVsixManifest,
  saveImportedVsixManifest,
} from "./vsix-import-store";
import type { VsixInspection } from "./vsix.types";
import type { ExtensionManifest } from "../types/extension-manifest";

export interface VsixImportResult {
  ok: boolean;
  extensionId?: string;
  displayName?: string;
  /** "persisted" imports survive restarts; "session" imports do not. */
  persistence: "persisted" | "session" | "none";
  warnings: string[];
  issues: Array<{ code: string; message: string; path?: string }>;
}

/**
 * Inspects the VSIX at `path` and imports its static contributions. Returns
 * `ok: false` with the Core issues when nothing importable was found.
 */
export async function importVsixFromPath(path: string): Promise<VsixImportResult> {
  const inspection = await invoke<VsixInspection>("extensions_inspect_vsix", { path });
  if (!inspection.installable) {
    return {
      ok: false,
      persistence: "none",
      warnings: [],
      issues: inspection.issues,
    };
  }

  const { manifest, warnings } = mapVsixInspectionToManifest(inspection);
  const registered = registerImportedManifest(manifest);
  const { persisted } = saveImportedVsixManifest(manifest);

  return {
    ok: true,
    extensionId: manifest.id,
    displayName: manifest.displayName,
    persistence: persisted ? "persisted" : "session",
    warnings,
    issues: inspection.issues,
  };
}

function registerImportedManifest(manifest: ExtensionManifest): Promise<void> {
  extensionRegistry.registerExtension(manifest, {
    isBundled: true,
    isEnabled: true,
    state: "installed",
  });
  return activateExtensionContributions(manifest.id, manifest);
}

/** Re-registers previously imported VSIX extensions after an app restart. */
export async function restoreImportedVsixExtensions(): Promise<void> {
  for (const manifest of readImportedVsixManifests()) {
    try {
      await registerImportedManifest(manifest);
    } catch (error) {
      console.error("Failed to restore imported VSIX extension:", manifest.id, error);
    }
  }
}

/** Deactivates and forgets one imported VSIX extension. */
export async function removeImportedVsixExtension(extensionId: string): Promise<void> {
  const manifest = readImportedVsixManifests().find((entry) => entry.id === extensionId);
  if (manifest) {
    await deactivateExtensionContributions(extensionId, manifest);
  }
  extensionRegistry.unregisterExtension(extensionId);
  removeImportedVsixManifest(extensionId);
}

export { extensionIdForVsix };
