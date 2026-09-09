import { describe, expect, test } from "bun:test";

import {
  extensionIdForVsix,
  mapVsixInspectionToManifest,
} from "./vsix-contribution-mapper";
import type { VsixInspection } from "./vsix.types";

function baseInspection(overrides: Partial<VsixInspection> = {}): VsixInspection {
  return {
    identity: {
      id: "zhuangtongfa.one-dark-pro",
      name: "one-dark-pro",
      displayName: "One Dark Pro",
      version: "2.3.0",
      publisher: "zhuangtongfa",
      description: "Atom's One Dark theme",
    },
    hasExecutable: false,
    installable: true,
    themes: [],
    iconThemes: [],
    snippets: [],
    languages: [],
    grammars: [],
    issues: [],
    ...overrides,
  };
}

describe("vsix contribution mapper", () => {
  test("maps themes with workbench colors and token colors", () => {
    const { manifest, warnings } = mapVsixInspectionToManifest(
      baseInspection({
        themes: [
          {
            label: "One Dark",
            path: "extension/themes/one-dark.json",
            appearance: "dark",
            contents: {
              type: "dark",
              colors: {
                "editor.background": "#282c34",
                "editor.foreground": "#abb2bf",
                "sideBar.background": "#21252b",
              },
              tokenColors: [
                { scope: ["comment"], settings: { foreground: "#5c6370" } },
                { scope: "keyword.operator", settings: { foreground: "#c678dd" } },
                { scope: "string.quoted.double", settings: { foreground: "#98c379" } },
                {
                  scope: "entity.name.function",
                  settings: { foreground: "#61afef" },
                },
              ],
            },
          },
        ],
      }),
    );

    expect(manifest.id).toBe("vsix.zhuangtongfa.one-dark-pro");
    expect(manifest.displayName).toBe("One Dark Pro");
    expect(manifest.themes).toHaveLength(1);
    const theme = manifest.themes![0]!;
    expect(theme.appearance).toBe("dark");
    expect(theme.colors.background).toBe("#282c34");
    expect(theme.colors.foreground).toBe("#abb2bf");
    expect(theme.colors.surface).toBe("#21252b");
    expect(theme.syntax).toMatchObject({
      comment: "#5c6370",
      operator: "#c678dd",
      string: "#98c379",
      function: "#61afef",
    });
    expect(warnings).toHaveLength(0);
  });

  test("prefers later token color rules at equal specificity and notes includes", () => {
    const { manifest, warnings } = mapVsixInspectionToManifest(
      baseInspection({
        themes: [
          {
            label: "Light+",
            path: "extension/themes/light.json",
            appearance: "light",
            contents: {
              include: "./defaults.json",
              tokenColors: [
                { scope: "keyword", settings: { foreground: "#0000ff" } },
                { scope: "keyword.control", settings: { foreground: "#af00db" } },
              ],
            },
          },
        ],
      }),
    );

    // "keyword.control" is more specific for the keyword role and later rules
    // win ties, so the control color takes precedence.
    expect(manifest.themes![0]!.syntax).toMatchObject({ keyword: "#af00db" });
    expect(warnings.some((warning) => warning.includes("defaults.json"))).toBe(true);
  });

  test("inlines svg icon assets and maps lookup tables", () => {
    const { manifest } = mapVsixInspectionToManifest(
      baseInspection({
        iconThemes: [
          {
            label: "Seti Icons",
            path: "extension/icons/seti-icons.json",
            definition: {
              iconDefinitions: {
                file: { iconPath: "./file.svg" },
                folder: { iconPath: "./folder.svg" },
                fontIcon: { fontCharacter: "\\e001" },
              },
              fileExtensions: { js: "_js" },
              filenames: { "package.json": "_npm" },
              folderNames: { src: "_src" },
              file: "_file",
              folder: "_folder",
            },
            assets: {
              "extension/icons/file.svg": `<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"/>`,
              "extension/icons/folder.svg": "<svg xmlns='http://www.w3.org/2000/svg'/>",
            },
          },
        ],
      }),
    );

    const iconTheme = manifest.iconThemes![0]!;
    expect(iconTheme.iconDefinitions.file).toBe(
      '<svg xmlns="http://www.w3.org/2000/svg"/>',
    );
    expect(iconTheme.iconDefinitions.folder).toBe(
      "<svg xmlns='http://www.w3.org/2000/svg'/>",
    );
    expect(iconTheme.iconDefinitions.fontIcon).toBeUndefined();
    // VS Code stores file-extension keys without the dot; the runtime
    // normalizer adds it at lookup time.
    expect(iconTheme.fileExtensions).toMatchObject({ js: "_js" });
    expect(iconTheme.filenames).toMatchObject({ "package.json": "_npm" });
    expect(iconTheme.folders).toMatchObject({ src: "_src" });
    expect(iconTheme.defaultFile).toBe("_file");
  });

  test("explodes scoped snippet collections per language", () => {
    const { manifest } = mapVsixInspectionToManifest(
      baseInspection({
        snippets: [
          {
            language: "rust",
            path: "extension/snippets/rust.json",
            snippets: {
              main: { prefix: "main", body: ["fn main() {", "\t$0", "}"] },
            },
          },
          {
            path: "extension/snippets/shared.code-snippets",
            snippets: {
              log: {
                prefix: "log",
                body: "console.log($1)",
                scope: "typescript,javascript",
                description: "Log",
              },
            },
          },
        ],
      }),
    );

    const languages = manifest.snippets!.map((contribution) => contribution.language).sort();
    expect(languages).toEqual(["javascript", "rust", "typescript"]);
    const rust = manifest.snippets!.find((entry) => entry.language === "rust")!;
    expect(rust.snippets[0]).toMatchObject({ prefix: "main" });
    const typescript = manifest.snippets!.find(
      (entry) => entry.language === "typescript",
    )!;
    expect(typescript.snippets[0]).toMatchObject({
      prefix: "log",
      body: "console.log($1)",
      description: "Log",
    });
  });

  test("maps languages and surfaces core issues as warnings", () => {
    const { manifest, warnings } = mapVsixInspectionToManifest(
      baseInspection({
        languages: [
          {
            id: "demo",
            extensions: [".demo"],
            aliases: ["Demo"],
            configuration: { comments: { lineComment: "//" } },
          },
        ],
        grammars: [
          { language: "demo", scopeName: "source.demo", path: "x.tmLanguage", format: "plist" },
        ],
        hasExecutable: true,
        issues: [
          {
            code: "grammarNotSupported",
            message: "TextMate grammars cannot be converted to tree-sitter parsers.",
          },
          {
            code: "executableNotSupported",
            message: "This extension ships JavaScript code.",
          },
        ],
      }),
    );

    expect(manifest.languages![0]).toMatchObject({ id: "demo", extensions: [".demo"] });
    expect(warnings.some((warning) => warning.includes("TextMate"))).toBe(true);
    expect(warnings.some((warning) => warning.includes("JavaScript"))).toBe(true);
    expect(extensionIdForVsix("a.b")).toBe("vsix.a.b");
  });
});
