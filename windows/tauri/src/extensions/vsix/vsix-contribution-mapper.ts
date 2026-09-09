/**
 * Maps a normalized VSIX inspection from the shared Rust Core into Lithe
 * `ExtensionManifest` contributions.
 *
 * The mapping is deliberately conservative: color themes, icon themes with
 * inline-able SVG assets, snippets, and language metadata convert directly,
 * while anything that would require the VS Code extension host or a TextMate
 * engine is dropped and surfaced as a warning instead.
 */

import type {
  ExtensionManifest,
  IconThemeContribution,
  LanguageContribution,
  Snippet,
  SnippetContribution,
  ThemeContribution,
} from "../types/extension-manifest";
import type { VsixIconTheme, VsixInspection, VsixTheme } from "./vsix.types";

/** Lithe editor syntax roles accepted by `ThemeContribution.syntax`. */
const SYNTAX_ROLES = [
  "comment",
  "keyword",
  "string",
  "number",
  "function",
  "variable",
  "tag",
  "attribute",
  "punctuation",
  "constant",
  "property",
  "type",
  "operator",
  "boolean",
  "null",
  "regex",
] as const;

type SyntaxRole = (typeof SYNTAX_ROLES)[number];

/**
 * TextMate scope prefixes each Lithe syntax role typically matches. Order
 * inside each list is the matching precedence for that role.
 */
const ROLE_SCOPES: Record<SyntaxRole, string[]> = {
  comment: ["comment.block.documentation", "comment"],
  string: ["string"],
  number: ["constant.numeric"],
  keyword: ["keyword"],
  operator: ["keyword.operator"],
  variable: ["variable"],
  property: ["support.type.property", "variable.other.property", "meta.property"],
  function: ["entity.name.function", "support.function", "meta.function-call"],
  type: ["entity.name.type", "entity.name.class", "entity.other.inherited-class", "support.type"],
  constant: ["constant.other", "constant.character"],
  boolean: ["constant.language.boolean"],
  null: ["constant.language.null"],
  regex: ["string.regexp"],
  tag: ["entity.name.tag"],
  attribute: ["entity.other.attribute-name"],
  punctuation: ["punctuation"],
};

/** VS Code workbench color keys that map onto Lithe app color slots. */
const WORKBENCH_COLOR_MAP: Record<string, string[]> = {
  background: ["editor.background"],
  foreground: ["editor.foreground"],
  surface: ["sideBar.background", "editorWidget.background"],
  border: ["panel.border", "editorWidget.border", "sideBarSectionHeader.border"],
  accent: ["focusBorder", "activityBar.foreground"],
  primary: ["activityBar.foreground", "focusBorder"],
  selected: ["list.activeSelectionBackground", "editor.selectionBackground"],
  "muted-foreground": ["descriptionForeground", "editorLineNumber.foreground"],
  "subtle-foreground": ["editorLineNumber.foreground", "descriptionForeground"],
};

interface TokenColor {
  scope?: string | string[];
  settings?: { foreground?: string };
}

export interface MappedVsixExtension {
  manifest: ExtensionManifest;
  /** Human-readable notes about contributions that could not be converted. */
  warnings: string[];
}

export function extensionIdForVsix(identityId: string): string {
  return `vsix.${identityId}`;
}

export function mapVsixInspectionToManifest(inspection: VsixInspection): MappedVsixExtension {
  const warnings: string[] = [];
  for (const issue of inspection.issues) {
    warnings.push(issue.path ? `${issue.message} (${issue.path})` : issue.message);
  }

  const manifest: ExtensionManifest = {
    id: extensionIdForVsix(inspection.identity.id),
    name: inspection.identity.name,
    displayName: inspection.identity.displayName || inspection.identity.name,
    description: inspection.identity.description || "",
    version: inspection.identity.version,
    publisher: inspection.identity.publisher || "vsix-import",
    categories: [],
    engines: {},
    iconThemes: inspection.iconThemes.map((theme) =>
      mapIconTheme(theme, warnings),
    ),
    languages: inspection.languages.map(
      (language): LanguageContribution => ({
        id: language.id,
        extensions: language.extensions,
        aliases: language.aliases,
      }),
    ),
    snippets: mapSnippets(inspection, warnings),
    themes: inspection.themes.map((theme) => mapTheme(theme, warnings)),
  };

  return { manifest, warnings };
}

function mapTheme(theme: VsixTheme, warnings: string[]): ThemeContribution {
  const contents = theme.contents;
  const colors = convertWorkbenchColors(
    isRecord(contents.colors) ? contents.colors : {},
  );
  const syntax = convertTokenColors(contents.tokenColors);

  if (typeof contents.include === "string") {
    warnings.push(
      `Theme "${theme.label}" inherits from "${contents.include}", which is not resolved during import.`,
    );
  }

  return {
    id: `${theme.label.toLowerCase().replace(/\s+/g, "-")}`,
    name: theme.label,
    appearance: theme.appearance,
    colors,
    syntax: Object.keys(syntax).length > 0 ? syntax : undefined,
  };
}

function convertWorkbenchColors(colors: Record<string, unknown>): Record<string, string> {
  const converted: Record<string, string> = {};
  for (const [role, sources] of Object.entries(WORKBENCH_COLOR_MAP)) {
    for (const source of sources) {
      const value = colors[source];
      if (typeof value === "string" && value.length > 0) {
        converted[role] = value;
        break;
      }
    }
  }
  return converted;
}

/**
 * Converts VS Code `tokenColors` rules into per-role colors. Each role takes
 * the color of the most specific matching scope; later rules win ties, which
 * mirrors VS Code's rule ordering closely enough for theme previews.
 */
function convertTokenColors(tokenColors: unknown): Record<string, string> {
  const rules = Array.isArray(tokenColors) ? (tokenColors as TokenColor[]) : [];
  const best = new Map<SyntaxRole, { specificity: number; order: number; color: string }>();

  rules.forEach((rule, order) => {
    const color = rule.settings?.foreground;
    if (typeof color !== "string" || color.length === 0) {
      return;
    }
    const scopes = Array.isArray(rule.scope)
      ? rule.scope
      : typeof rule.scope === "string"
        ? rule.scope.split(",").map((scope) => scope.trim())
        : [];
    for (const role of SYNTAX_ROLES) {
      for (const roleScope of ROLE_SCOPES[role]) {
        for (const scope of scopes) {
          const specificity = matchSpecificity(scope, roleScope);
          if (specificity === null) {
            continue;
          }
          const existing = best.get(role);
          if (!existing || specificity >= existing.specificity) {
            best.set(role, { specificity, order, color });
          }
        }
      }
    }
  });

  const syntax: Record<string, string> = {};
  for (const [role, match] of best) {
    syntax[role] = match.color;
  }
  return syntax;
}

/**
 * Returns how specifically `scope` matches `roleScope`, or null when it does
 * not match. Longer matching selectors win, mirroring TextMate's longest-
 * prefix rule; ties fall through so later rules can override earlier ones.
 */
function matchSpecificity(scope: string, roleScope: string): number | null {
  const normalizedScope = scope.trim();
  if (!normalizedScope) {
    return null;
  }
  const wildcard = roleScope.endsWith(".*") ? roleScope.slice(0, -2) : null;
  if (normalizedScope === roleScope) {
    return roleScope.length;
  }
  if (normalizedScope.startsWith(`${roleScope}.`)) {
    return roleScope.length;
  }
  if (wildcard && normalizedScope.startsWith(`${wildcard}.`)) {
    return wildcard.length;
  }
  return null;
}

function mapIconTheme(theme: VsixIconTheme, warnings: string[]): IconThemeContribution {
  const definition = theme.definition;
  const rawDefinitions = isRecord(definition.iconDefinitions)
    ? definition.iconDefinitions
    : {};
  const iconDefinitions: Record<string, string> = {};
  for (const [key, value] of Object.entries(rawDefinitions)) {
    if (!isRecord(value)) {
      continue;
    }
    const iconPath = value.iconPath;
    if (typeof iconPath !== "string") {
      continue;
    }
    const asset = resolveAssetPath(theme, iconPath);
    if (asset) {
      iconDefinitions[key] = asset;
    } else {
      warnings.push(`Icon "${key}" (${iconPath}) has no importable SVG asset.`);
    }
  }

  // VS Code splits folder names into plain string keys plus an `expanded`
  // sub-object; Lithe keeps them as separate lookup tables.
  const folderNames = isRecord(definition.folderNames) ? definition.folderNames : {};
  const { expanded: expandedFolderNames, ...plainFolderNames } = folderNames;

  return {
    id: theme.label.toLowerCase().replace(/\s+/g, "-"),
    name: theme.label,
    iconDefinitions,
    fileExtensions: stringRecord(definition.fileExtensions),
    filenames: stringRecord(definition.filenames),
    folders: {
      ...stringRecord(plainFolderNames),
      ...stringRecord(definition.folders),
    },
    expandedFolders: stringRecord(expandedFolderNames),
    defaultFile: singleString(definition.file),
    defaultFolder: singleString(definition.folder),
  };
}

/** Resolves an iconPath against the theme JSON directory, mirroring VS Code. */
function resolveAssetPath(theme: VsixIconTheme, iconPath: string): string | undefined {
  const normalized = iconPath.replace(/\\/g, "/").replace(/^\.\//, "");
  const themeDirectory = theme.path.includes("/")
    ? theme.path.slice(0, theme.path.lastIndexOf("/") + 1)
    : "";
  const candidates = [`${themeDirectory}${normalized}`, normalized];
  for (const candidate of candidates) {
    const asset = theme.assets[candidate];
    if (typeof asset === "string") {
      return stripSvgProlog(asset);
    }
  }
  return undefined;
}

/** Removes XML declarations and doctypes so the value starts with `<svg`. */
function stripSvgProlog(svg: string): string {
  return svg.replace(/<\?xml[^>]*\?>/g, "").replace(/<!DOCTYPE[^>]*>/g, "").trim();
}

function mapSnippets(inspection: VsixInspection, warnings: string[]): SnippetContribution[] {
  const contributions: SnippetContribution[] = [];
  for (const collection of inspection.snippets) {
    const languages = new Set<string>();
    if (collection.language) {
      languages.add(collection.language);
    }
    const snippets: Snippet[] = [];
    for (const value of Object.values(collection.snippets)) {
      const prefix = value.prefix;
      const body = value.body;
      if (typeof prefix !== "string" || prefix.length === 0) {
        continue;
      }
      if (typeof body !== "string" && !Array.isArray(body)) {
        continue;
      }
      const scope = typeof value.scope === "string" ? value.scope : undefined;
      for (const language of scope ? scope.split(",").map((part) => part.trim()) : [""]) {
        if (language) {
          languages.add(language);
        }
      }
      snippets.push({
        prefix,
        body,
        description: typeof value.description === "string" ? value.description : undefined,
        scope,
      });
    }
    if (snippets.length === 0) {
      continue;
    }
    const targetLanguages = languages.size > 0 ? Array.from(languages) : ["plaintext"];
    for (const language of targetLanguages) {
      contributions.push({ language, snippets });
    }
  }
  if (contributions.length === 0 && inspection.snippets.length > 0) {
    warnings.push("Snippet collections contained no usable prefixes.");
  }
  return contributions;
}

function stringRecord(value: unknown): Record<string, string> | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const entries = Object.entries(value).filter(
    (entry): entry is [string, string] => typeof entry[1] === "string",
  );
  return entries.length > 0 ? Object.fromEntries(entries) : undefined;
}

function singleString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
