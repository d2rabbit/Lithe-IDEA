//! VSIX (Visual Studio Code extension package) inspection.
//!
//! Reads the standard `extension.vsixmanifest` identity and the embedded
//! `extension/package.json`, then extracts the static contribution points that
//! Lithe can adopt (color themes, icon themes, snippets, language metadata) and
//! reports everything it cannot adopt (JavaScript hosts, TextMate grammars) as
//! structured issues. The archive is treated as untrusted input: entry paths
//! are normalized against zip-slip, parsed JSON entries are size-capped, and
//! no entry is ever written to disk or executed.

use crate::protocol::{CoreError, ErrorCode};
use crate::protocol::{
    VsixGrammarResponse, VsixIconThemeResponse, VsixIdentityResponse, VsixInspectResponse,
    VsixIssueResponse, VsixLanguageResponse, VsixSnippetResponse, VsixThemeResponse,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::File;
use zip::ZipArchive;

/// Maximum bytes read from any single parsed JSON contribution entry.
const MAX_JSON_ENTRY_BYTES: u64 = 2 * 1024 * 1024;
/// Maximum bytes read across all entries of one inspection.
const MAX_TOTAL_READ_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum bytes collected for one icon asset; larger or non-UTF-8 assets are
/// reported instead of being imported.
const MAX_ICON_ASSET_BYTES: u64 = 128 * 1024;
/// Maximum bytes collected across all icon assets of one icon theme.
const MAX_ICON_THEME_ASSET_BYTES: u64 = 512 * 1024;
/// Maximum entries visited; hostile archives with unbounded members stop early.
const MAX_VISITED_ENTRIES: usize = 8_192;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Absolute archive path selected by the platform file picker.
pub struct VsixInspectRequest {
    pub path: String,
}

/// Inspects one VSIX archive and returns its identity and portable
/// contributions without extracting anything to disk.
pub fn inspect_vsix(request: VsixInspectRequest) -> Result<VsixInspectResponse, CoreError> {
    if request.path.trim().is_empty() {
        return Err(CoreError::new(
            ErrorCode::InvalidRequest,
            "VSIX inspection requires an archive path.",
        ));
    }
    let file = File::open(&request.path).map_err(|error| {
        CoreError::new(ErrorCode::ParseFailed, "Could not read VSIX archive")
            .with_details(error.to_string())
    })?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        CoreError::new(ErrorCode::ParseFailed, "File is not a valid VSIX archive")
            .with_details(error.to_string())
    })?;

    let mut inspection = Inspection::default();
    let manifest_index = (0..archive.len())
        .find(|index| {
            archive
                .by_index(*index)
                .is_ok_and(|entry| normalized_entry_name(&entry) == "extension.vsixmanifest")
        })
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::ParseFailed,
                "VSIX archive is missing extension.vsixmanifest",
            )
        })?;
    let identity = {
        let entry = archive.by_index(manifest_index).map_err(read_error)?;
        read_entry(entry, "extension.vsixmanifest", &mut inspection)
            .map_err(required_entry_error)
            .map(|text| parse_vsix_manifest_identity(&text))
    }?;

    let package_json: PackageJson = {
        let index = find_entry(&mut archive, "extension/package.json").ok_or_else(|| {
            CoreError::new(
                ErrorCode::ParseFailed,
                "VSIX archive is missing extension/package.json",
            )
        })?;
        let entry = archive.by_index(index).map_err(read_error)?;
        let text = read_entry(entry, "extension/package.json", &mut inspection)
            .map_err(required_entry_error)?;
        serde_json::from_str(&text).map_err(|error| {
            CoreError::new(ErrorCode::ParseFailed, "Invalid extension package.json")
                .with_details(error.to_string())
        })?
    };

    let mut result = VsixInspectResponse {
        identity: merge_identity(package_json.identity(), identity),
        has_executable: package_json.main.is_some() || package_json.browser.is_some(),
        installable: false,
        themes: Vec::new(),
        icon_themes: Vec::new(),
        snippets: Vec::new(),
        languages: Vec::new(),
        grammars: Vec::new(),
        issues: Vec::new(),
    };
    if result.has_executable {
        result.issues.push(VsixIssueResponse {
            code: "executableNotSupported".to_string(),
            message: "This extension ships JavaScript code; Lithe imports only its static \
                      contributions."
                .to_string(),
            path: None,
        });
    }

    if let Some(contributes) = package_json.contributes {
        collect_contributes(&mut archive, &mut inspection, &mut result, contributes);
    }
    if result.themes.is_empty()
        && result.icon_themes.is_empty()
        && result.snippets.is_empty()
        && result.languages.is_empty()
    {
        result.issues.push(VsixIssueResponse {
            code: "noSupportedContributions".to_string(),
            message: "No importable theme, icon theme, snippet, or language contributions were \
                      found."
                .to_string(),
            path: None,
        });
    }
    result.installable = !result.themes.is_empty()
        || !result.icon_themes.is_empty()
        || !result.snippets.is_empty()
        || !result.languages.is_empty();
    Ok(result)
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PackageJson {
    #[serde(default)]
    name: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    main: Option<String>,
    #[serde(default)]
    browser: Option<String>,
    #[serde(default)]
    contributes: Option<PackageContributes>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PackageContributes {
    #[serde(default)]
    themes: Vec<ThemePoint>,
    #[serde(default)]
    icon_themes: Vec<IconThemePoint>,
    #[serde(default)]
    snippets: Vec<SnippetPoint>,
    #[serde(default)]
    languages: Vec<LanguagePoint>,
    #[serde(default)]
    grammars: Vec<GrammarPoint>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThemePoint {
    label: String,
    path: String,
    #[serde(default)]
    ui_theme: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IconThemePoint {
    label: String,
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnippetPoint {
    #[serde(default)]
    language: Option<String>,
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanguagePoint {
    id: String,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    configuration: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GrammarPoint {
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    scope_name: Option<String>,
    path: String,
}

impl PackageJson {
    /// Builds the package-derived identity, keeping fields the VSIX manifest
    /// may not carry.
    fn identity(&self) -> VsixIdentityResponse {
        VsixIdentityResponse {
            id: match (&self.publisher, &self.name) {
                (Some(publisher), name) if !name.is_empty() => format!("{publisher}.{name}"),
                _ => self.name.clone(),
            },
            name: self.name.clone(),
            display_name: self.display_name.clone(),
            version: self.version.clone().unwrap_or_default(),
            publisher: self.publisher.clone(),
            description: self.description.clone(),
        }
    }
}

#[derive(Default)]
struct Inspection {
    total_read_bytes: u64,
}

fn merge_identity(
    package: VsixIdentityResponse,
    manifest: VsixIdentityResponse,
) -> VsixIdentityResponse {
    VsixIdentityResponse {
        // The package.json identity is the one VS Code tooling maintains; the
        // VSIX manifest only fills fields the package omits.
        id: if package.id.is_empty() {
            manifest.id
        } else {
            package.id
        },
        name: if package.name.is_empty() {
            manifest.name
        } else {
            package.name
        },
        display_name: package.display_name.or(manifest.display_name),
        version: if package.version.is_empty() {
            manifest.version
        } else {
            package.version
        },
        publisher: package.publisher.or(manifest.publisher),
        description: package.description.or(manifest.description),
    }
}

fn collect_contributes(
    archive: &mut ZipArchive<File>,
    inspection: &mut Inspection,
    result: &mut VsixInspectResponse,
    contributes: PackageContributes,
) {
    for theme in contributes.themes {
        let Some(path) = package_entry_path(&theme.path) else {
            result.issues.push(outside_package_issue(&theme.path));
            continue;
        };
        let Some(contents) = read_json_entry(archive, inspection, result, &path) else {
            continue;
        };
        result.themes.push(VsixThemeResponse {
            label: theme.label,
            path,
            appearance: theme_appearance(theme.ui_theme.as_deref()).to_string(),
            contents,
        });
    }
    for icon_theme in contributes.icon_themes {
        let Some(path) = package_entry_path(&icon_theme.path) else {
            result.issues.push(outside_package_issue(&icon_theme.path));
            continue;
        };
        let Some(definition) = read_json_entry(archive, inspection, result, &path) else {
            continue;
        };
        let base_directory = theme_directory(&path);
        let assets = collect_icon_assets(archive, inspection, result, &definition, &base_directory);
        result.icon_themes.push(VsixIconThemeResponse {
            label: icon_theme.label,
            path,
            definition,
            assets,
        });
    }
    for snippet in contributes.snippets {
        let Some(path) = package_entry_path(&snippet.path) else {
            result.issues.push(outside_package_issue(&snippet.path));
            continue;
        };
        let Some(snippets) = read_json_entry(archive, inspection, result, &path) else {
            continue;
        };
        result.snippets.push(VsixSnippetResponse {
            language: snippet.language,
            path,
            snippets,
        });
    }
    for language in contributes.languages {
        let configuration = language
            .configuration
            .as_deref()
            .and_then(|configuration| package_entry_path(configuration));
        let configuration =
            configuration.and_then(|path| read_json_entry(archive, inspection, result, &path));
        result.languages.push(VsixLanguageResponse {
            id: language.id,
            extensions: language.extensions,
            aliases: language.aliases,
            configuration,
        });
    }
    for grammar in contributes.grammars {
        // TextMate grammars drive a different highlighting engine than Lithe's
        // tree-sitter pipeline, so they are reported instead of imported.
        let format = if grammar.path.to_ascii_lowercase().ends_with(".json") {
            "json"
        } else {
            "plist"
        };
        result.grammars.push(VsixGrammarResponse {
            language: grammar.language,
            scope_name: grammar.scope_name,
            path: grammar.path,
            format: format.to_string(),
        });
    }
    if !result.grammars.is_empty() {
        result.issues.push(VsixIssueResponse {
            code: "grammarNotSupported".to_string(),
            message: "TextMate grammars cannot be converted to tree-sitter parsers; this \
                      extension will not provide syntax highlighting."
                .to_string(),
            path: None,
        });
    }
}

/// Collects UTF-8 SVG assets referenced by an icon theme definition. Font-based
/// icon definitions are skipped and reported once per theme.
/// Directory (inside `extension/`) that relative icon asset paths resolve
/// against, mirroring VS Code's theme-relative resolution.
fn theme_directory(path: &str) -> String {
    let relative = path.strip_prefix("extension/").unwrap_or(path);
    match relative.rfind('/') {
        Some(index) => relative[..=index].to_string(),
        None => String::new(),
    }
}

fn collect_icon_assets(
    archive: &mut ZipArchive<File>,
    inspection: &mut Inspection,
    result: &mut VsixInspectResponse,
    definition: &Value,
    base_directory: &str,
) -> BTreeMap<String, String> {
    let mut assets = BTreeMap::new();
    let mut total_bytes = 0u64;
    let Some(definitions) = definition.get("iconDefinitions").and_then(Value::as_object) else {
        return assets;
    };
    for (_, definition) in definitions {
        let Some(icon_path) = definition.get("iconPath").and_then(Value::as_str) else {
            continue;
        };
        let Some(path) = resolve_asset_path(icon_path, base_directory) else {
            result.issues.push(outside_package_issue(icon_path));
            continue;
        };
        if path.to_ascii_lowercase().ends_with(".svg") {
            // SVG assets are collected below.
        } else {
            result.issues.push(VsixIssueResponse {
                code: "fontIconNotSupported".to_string(),
                message: "Non-SVG icon definitions are not importable; only SVG icons are."
                    .to_string(),
                path: Some(path.clone()),
            });
            continue;
        }
        let index = match find_entry(archive, &path) {
            Some(index) => index,
            None => {
                result.issues.push(missing_entry_issue(&path));
                continue;
            }
        };
        let entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(error) => {
                result.issues.push(read_issue(&path, error));
                continue;
            }
        };
        if entry.size() > MAX_ICON_ASSET_BYTES {
            result.issues.push(VsixIssueResponse {
                code: "iconAssetTooLarge".to_string(),
                message: "Icon asset exceeds the import size limit.".to_string(),
                path: Some(path),
            });
            continue;
        }
        if total_bytes + entry.size() > MAX_ICON_THEME_ASSET_BYTES {
            result.issues.push(VsixIssueResponse {
                code: "iconAssetTooLarge".to_string(),
                message: "Icon theme exceeds the total asset import limit.".to_string(),
                path: Some(path),
            });
            continue;
        }
        let mut bytes = Vec::new();
        let mut entry = entry;
        if std::io::Read::read_to_end(&mut entry, &mut bytes).is_err() {
            result.issues.push(VsixIssueResponse {
                code: "contributionUnreadable".to_string(),
                message: "Icon asset could not be read.".to_string(),
                path: Some(path),
            });
            continue;
        }
        inspection.total_read_bytes += bytes.len() as u64;
        match String::from_utf8(bytes) {
            Ok(text) => {
                total_bytes += text.len() as u64;
                assets.insert(path, text);
            }
            Err(_) => result.issues.push(VsixIssueResponse {
                code: "iconAssetBinary".to_string(),
                message: "Icon asset is not UTF-8 text and was skipped.".to_string(),
                path: Some(path),
            }),
        }
    }
    assets
}

fn read_json_entry(
    archive: &mut ZipArchive<File>,
    inspection: &mut Inspection,
    result: &mut VsixInspectResponse,
    path: &str,
) -> Option<Value> {
    let index = find_entry(archive, path)?;
    let entry = match archive.by_index(index) {
        Ok(entry) => entry,
        Err(error) => {
            result.issues.push(read_issue(path, error));
            return None;
        }
    };
    let text = match read_entry(entry, path, inspection) {
        Ok(text) => text,
        Err(issue) => {
            result.issues.push(issue);
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(error) => {
            result.issues.push(VsixIssueResponse {
                code: "contributionUnreadable".to_string(),
                message: format!("Contribution JSON is invalid: {error}"),
                path: Some(path.to_string()),
            });
            None
        }
    }
}

/// Reads one entry with size and cumulative budget enforcement, returning an
/// issue instead of the text when the entry cannot contribute safely.
fn read_entry(
    mut entry: zip::read::ZipFile<'_>,
    path: &str,
    inspection: &mut Inspection,
) -> Result<String, VsixIssueResponse> {
    if entry.size() > MAX_JSON_ENTRY_BYTES {
        return Err(VsixIssueResponse {
            code: "entryTooLarge".to_string(),
            message: "Contribution entry exceeds the 2 MiB inspection limit.".to_string(),
            path: Some(path.to_string()),
        });
    }
    if inspection.total_read_bytes + entry.size() > MAX_TOTAL_READ_BYTES {
        return Err(VsixIssueResponse {
            code: "entryTooLarge".to_string(),
            message: "VSIX exceeds the total inspection budget.".to_string(),
            path: Some(path.to_string()),
        });
    }
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut bytes).map_err(|error| read_issue(path, error))?;
    inspection.total_read_bytes += bytes.len() as u64;
    String::from_utf8(bytes).map_err(|_| VsixIssueResponse {
        code: "contributionUnreadable".to_string(),
        message: "Contribution entry is not UTF-8 text.".to_string(),
        path: Some(path.to_string()),
    })
}

/// Normalizes an entry name to slash-separated form.
fn normalized_entry_name(entry: &zip::read::ZipFile<'_>) -> String {
    entry.name().replace('\\', "/")
}

/// Finds an entry index by normalized name; later duplicates lose to the first.
fn find_entry(archive: &mut ZipArchive<File>, name: &str) -> Option<usize> {
    let mut visited = 0usize;
    for index in 0..archive.len() {
        visited += 1;
        if visited > MAX_VISITED_ENTRIES {
            return None;
        }
        let entry = archive.by_index(index).ok()?;
        if normalized_entry_name(&entry) == name {
            return Some(index);
        }
    }
    None
}

/// Resolves a package.json contribution path against the `extension/` root,
/// rejecting paths that escape the package directory.
fn package_entry_path(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let mut parts: Vec<&str> = normalized
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.len() > 1 {
        // Drop the leading `extension/` a caller may already have supplied so
        // the root is added exactly once below.
        if parts[0] == "extension" {
            parts.remove(0);
        }
    }
    if parts.is_empty() || parts.iter().any(|part| *part == "..") || normalized.contains(':') {
        return None;
    }
    Some(format!("extension/{}", parts.join("/")))
}

/// Resolves an icon asset path relative to the theme JSON directory.
fn resolve_asset_path(path: &str, base_directory: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    if normalized.starts_with('/') {
        return package_entry_path(&normalized);
    }
    if base_directory.is_empty() {
        return package_entry_path(&normalized);
    }
    package_entry_path(&format!("{base_directory}{normalized}"))
}

/// Derives the portable light/dark appearance from the VS Code `uiTheme`.
fn theme_appearance(ui_theme: Option<&str>) -> &'static str {
    match ui_theme {
        Some("vs-dark") | Some("hc-black") => "dark",
        _ => "light",
    }
}

/// Extracts the identity fields from `extension.vsixmanifest` XML. Only the
/// known VSIX 2.0 manifest elements are read; unknown content is ignored.
fn parse_vsix_manifest_identity(xml: &str) -> VsixIdentityResponse {
    let mut identity = VsixIdentityResponse {
        id: String::new(),
        name: String::new(),
        display_name: None,
        version: String::new(),
        publisher: None,
        description: None,
    };
    let mut parser = quick_xml::Reader::from_str(xml);
    let mut buffer = Vec::new();
    let mut current: Option<String> = None;
    loop {
        buffer.clear();
        match parser.read_event_into(&mut buffer) {
            Ok(quick_xml::events::Event::Start(start)) => {
                let name = local_name(start.name().as_ref()).to_string();
                if name == "Identity" {
                    for attribute in start.attributes().flatten() {
                        let key = local_name(attribute.key.as_ref());
                        let value = String::from_utf8_lossy(&attribute.value).to_string();
                        match key.as_str() {
                            "Id" => identity.id = value,
                            "Version" => identity.version = value,
                            "Publisher" => identity.publisher = Some(value),
                            _ => {}
                        }
                    }
                }
                current = Some(name);
            }
            Ok(quick_xml::events::Event::Empty(start)) => {
                if local_name(start.name().as_ref()) == "Identity" {
                    for attribute in start.attributes().flatten() {
                        let key = local_name(attribute.key.as_ref());
                        let value = String::from_utf8_lossy(&attribute.value).to_string();
                        match key.as_str() {
                            "Id" => identity.id = value,
                            "Version" => identity.version = value,
                            "Publisher" => identity.publisher = Some(value),
                            _ => {}
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Text(text)) => {
                if let Some(element) = &current {
                    let Ok(raw) = text.unescape() else {
                        continue;
                    };
                    let value = raw.trim().to_string();
                    if value.is_empty() {
                        continue;
                    }
                    match element.as_str() {
                        "DisplayName" => identity.display_name = Some(value),
                        "Description" => {
                            identity.description.get_or_insert(value);
                        }
                        "Author" => {
                            identity.publisher.get_or_insert(value);
                        }
                        _ => {}
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) | Ok(_) => {}
        }
    }
    identity
}

/// Strips an XML qualified name down to its local part.
fn local_name(name: &[u8]) -> String {
    let text = String::from_utf8_lossy(name);
    text.rsplit(':').next().unwrap_or(&text).to_string()
}

fn outside_package_issue(path: &str) -> VsixIssueResponse {
    VsixIssueResponse {
        code: "pathOutsidePackage".to_string(),
        message: "Contribution path escapes the package directory and was skipped.".to_string(),
        path: Some(path.to_string()),
    }
}

fn missing_entry_issue(path: &str) -> VsixIssueResponse {
    VsixIssueResponse {
        code: "contributionUnreadable".to_string(),
        message: "Referenced entry is missing from the archive.".to_string(),
        path: Some(path.to_string()),
    }
}

fn read_issue(path: &str, error: impl std::fmt::Display) -> VsixIssueResponse {
    VsixIssueResponse {
        code: "contributionUnreadable".to_string(),
        message: format!("Entry could not be read: {error}"),
        path: Some(path.to_string()),
    }
}

/// Converts an issue on a required manifest entry into a hard failure.
fn required_entry_error(issue: VsixIssueResponse) -> CoreError {
    CoreError::new(ErrorCode::ParseFailed, issue.message)
        .with_details(issue.path.unwrap_or_default())
}

fn read_error(error: zip::result::ZipError) -> CoreError {
    CoreError::new(ErrorCode::ParseFailed, "Could not read VSIX entry")
        .with_details(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Builds an in-memory VSIX with the given name/content entries.
    fn build_vsix(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, content) in entries {
            zip.start_file(*name, options).expect("entry should start");
            zip.write_all(content.as_bytes())
                .expect("entry should write");
        }
        zip.finish().expect("zip should finish");
        buffer.into_inner()
    }

    fn package_json(contributes: &str) -> String {
        format!(
            r#"{{
                "name": "demo-pack",
                "displayName": "Demo Pack",
                "description": "Static demo extension",
                "version": "1.2.3",
                "publisher": "lithe",
                "engines": {{ "vscode": "^1.70.0" }},
                "contributes": {contributes}
            }}"#
        )
    }

    const VSIX_MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
        <PackageManifest Version="2.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011">
            <Metadata>
                <Identity Language="en-US" Id="demo-pack" Version="1.2.3" Publisher="lithe"/>
                <DisplayName>Demo Pack</DisplayName>
                <Description xml:space="preserve">Static demo extension</Description>
            </Metadata>
        </PackageManifest>"#;

    #[test]
    fn inspects_theme_snippet_and_language_contributions() {
        let theme = r##"{ "type": "dark", "colors": { "editor.background": "#282c34" }, "tokenColors": [ { "scope": "comment", "settings": { "foreground": "#5c6370" } } ] }"##;
        let snippets = r#"{ "main": { "prefix": "main", "body": ["fn main() {", "\t$0", "}"], "description": "Main function" } }"#;
        let language_configuration =
            r#"{ "comments": { "lineComment": "//" }, "brackets": [["{", "}"]] }"#;
        let contributes = format!(
            r#"{{
                "themes": [{{ "label": "Demo Dark", "path": "./themes/demo-dark-color-theme.json", "uiTheme": "vs-dark" }}],
                "snippets": [{{ "language": "rust", "path": "snippets/rust.json" }}],
                "languages": [{{ "id": "demo", "extensions": [".demo"], "aliases": ["Demo"], "configuration": "./language-configuration.json" }}],
                "grammars": [{{ "language": "demo", "scopeName": "source.demo", "path": "./syntaxes/demo.tmLanguage" }}]
            }}"#
        );
        let bytes = build_vsix(&[
            ("extension.vsixmanifest", VSIX_MANIFEST),
            ("extension/package.json", &package_json(&contributes)),
            ("extension/themes/demo-dark-color-theme.json", theme),
            ("extension/snippets/rust.json", snippets),
            (
                "extension/language-configuration.json",
                language_configuration,
            ),
            ("extension/syntaxes/demo.tmLanguage", "<plist></plist>"),
        ]);
        let archive = write_temp_vsix(bytes);
        let result = inspect_vsix(VsixInspectRequest {
            path: archive.path.to_string_lossy().to_string(),
        })
        .expect("inspection should succeed");

        assert_eq!(result.identity.id, "lithe.demo-pack");
        assert_eq!(result.identity.version, "1.2.3");
        assert_eq!(result.identity.display_name.as_deref(), Some("Demo Pack"));
        assert!(!result.has_executable);
        assert!(result.installable);

        assert_eq!(result.themes.len(), 1);
        assert_eq!(result.themes[0].appearance, "dark");
        assert_eq!(
            result.themes[0]
                .contents
                .get("colors")
                .and_then(|colors| colors.get("editor.background"))
                .and_then(Value::as_str),
            Some("#282c34")
        );
        assert_eq!(result.snippets.len(), 1);
        assert_eq!(result.snippets[0].language.as_deref(), Some("rust"));
        assert_eq!(result.languages.len(), 1);
        assert_eq!(result.languages[0].extensions, vec![".demo"]);
        assert!(result.languages[0].configuration.is_some());
        assert_eq!(result.grammars.len(), 1);
        assert_eq!(result.grammars[0].format, "plist");
        assert!(result
            .issues
            .iter()
            .any(|issue| issue.code == "grammarNotSupported"));
    }

    #[test]
    fn reports_executable_extensions_and_rejects_escaping_paths() {
        let contributes = r#"{
            "themes": [ { "label": "Evil", "path": "../outside/theme.json" } ],
            "iconThemes": [ { "label": "Icons", "path": "./icons/theme.json" } ]
        }"#;
        let icon_theme = r#"{ "iconDefinitions": { "file": { "iconPath": "./file.svg" }, "folder": { "iconPath": "./folder.svg" } } }"#;
        let bytes = build_vsix(&[
            ("extension.vsixmanifest", VSIX_MANIFEST),
            ("extension/package.json", &package_json(contributes)),
            ("extension/icons/theme.json", icon_theme),
            (
                "extension/icons/file.svg",
                "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
            ),
        ]);
        let archive = write_temp_vsix(bytes);
        let result = inspect_vsix(VsixInspectRequest {
            path: archive.path.to_string_lossy().to_string(),
        })
        .expect("inspection should succeed");

        assert!(result.themes.is_empty());
        assert!(result
            .issues
            .iter()
            .any(|issue| issue.code == "pathOutsidePackage"));
        assert_eq!(result.icon_themes.len(), 1);
        let assets = &result.icon_themes[0].assets;
        assert_eq!(assets.len(), 1);
        assert!(assets.contains_key("extension/icons/file.svg"));
    }

    #[test]
    fn rejects_corrupt_archives_and_missing_manifests() {
        let archive = write_temp_vsix(b"not a zip file".to_vec());
        let error = inspect_vsix(VsixInspectRequest {
            path: archive.path.to_string_lossy().to_string(),
        })
        .expect_err("corrupt archive must fail");
        assert!(matches!(error.code, ErrorCode::ParseFailed));

        let bytes = build_vsix(&[("other/file.txt", "hello")]);
        let archive = write_temp_vsix(bytes);
        let error = inspect_vsix(VsixInspectRequest {
            path: archive.path.to_string_lossy().to_string(),
        })
        .expect_err("missing manifest must fail");
        assert!(matches!(error.code, ErrorCode::ParseFailed));
    }

    struct TempArchive {
        path: std::path::PathBuf,
    }

    impl Drop for TempArchive {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    /// Deterministic tests may use clearly fake temporary paths; the file is
    /// removed when the guard drops.
    fn write_temp_vsix(bytes: Vec<u8>) -> TempArchive {
        let mut path = std::env::temp_dir();
        path.push(format!("lithe-vsix-test-{}.vsix", std::process::id()));
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        path.set_file_name(format!(
            "lithe-vsix-test-{}-{}.vsix",
            std::process::id(),
            sequence
        ));
        std::fs::write(&path, bytes).expect("temp vsix should write");
        TempArchive { path }
    }
}
