/**
 * Types mirroring the deterministic `extensions.inspectVsix` response from the
 * shared Rust Core. Keep field names in sync with
 * `rust/lithe-core/src/protocol/contracts.rs`.
 */

export interface VsixIdentity {
  id: string;
  name: string;
  displayName?: string;
  version: string;
  publisher?: string;
  description?: string;
}

export interface VsixTheme {
  label: string;
  path: string;
  appearance: "dark" | "light";
  contents: Record<string, unknown>;
}

export interface VsixIconTheme {
  label: string;
  path: string;
  definition: Record<string, unknown>;
  /** UTF-8 SVG asset contents keyed by archive path (`extension/...`). */
  assets: Record<string, string>;
}

export interface VsixSnippetCollection {
  language?: string;
  path: string;
  snippets: Record<string, Record<string, unknown>>;
}

export interface VsixLanguage {
  id: string;
  extensions: string[];
  aliases: string[];
  configuration?: Record<string, unknown>;
}

export interface VsixGrammar {
  language?: string;
  scopeName?: string;
  path: string;
  format: string;
}

export interface VsixIssue {
  code: string;
  message: string;
  path?: string;
}

export interface VsixInspection {
  identity: VsixIdentity;
  hasExecutable: boolean;
  installable: boolean;
  themes: VsixTheme[];
  iconThemes: VsixIconTheme[];
  snippets: VsixSnippetCollection[];
  languages: VsixLanguage[];
  grammars: VsixGrammar[];
  issues: VsixIssue[];
}
