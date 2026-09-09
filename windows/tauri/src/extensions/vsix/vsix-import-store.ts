/**
 * Local persistence for VSIX extensions imported at runtime.
 *
 * Imported VSIX extensions are static contribution packages: their whole
 * mapped manifest (including inline SVG assets) is small enough for
 * localStorage, so no native extension-directory backend is required.
 */

import type { ExtensionManifest } from "../types/extension-manifest";

const IMPORTED_VSIX_KEY = "lithe.importedVsixExtensions";
/** Keep imports comfortably below the ~5 MiB localStorage budget. */
const MAX_PERSISTED_BYTES = 2 * 1024 * 1024;

function canUseStorage(): boolean {
  return typeof window !== "undefined" && typeof window.localStorage !== "undefined";
}

export function readImportedVsixManifests(): ExtensionManifest[] {
  if (!canUseStorage()) {
    return [];
  }
  try {
    const raw = window.localStorage.getItem(IMPORTED_VSIX_KEY);
    const parsed = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? (parsed as ExtensionManifest[]) : [];
  } catch {
    return [];
  }
}

export function saveImportedVsixManifest(manifest: ExtensionManifest): { persisted: boolean } {
  const manifests = readImportedVsixManifests().filter((entry) => entry.id !== manifest.id);
  manifests.push(manifest);
  if (!canUseStorage()) {
    return { persisted: false };
  }
  const serialized = JSON.stringify(manifests);
  if (serialized.length > MAX_PERSISTED_BYTES) {
    return { persisted: false };
  }
  try {
    window.localStorage.setItem(IMPORTED_VSIX_KEY, serialized);
    return { persisted: true };
  } catch {
    return { persisted: false };
  }
}

export function removeImportedVsixManifest(extensionId: string): void {
  if (!canUseStorage()) {
    return;
  }
  const manifests = readImportedVsixManifests().filter((entry) => entry.id !== extensionId);
  try {
    window.localStorage.setItem(IMPORTED_VSIX_KEY, JSON.stringify(manifests));
  } catch {
    // Removing must not fail the uninstall flow when the quota is exhausted.
  }
}
