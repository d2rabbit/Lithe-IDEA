//! Deterministic Kotlin, Scala, and Groovy syntax and fold classification for
//! native editor renderers, mirroring the Java recognition quality bar.

use crate::protocol::{CoreError, ErrorCode};
use crate::protocol::{
    JavaFoldRegionResponse, JavaSyntaxHighlightResponse, LanguageStructureResponse,
};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashSet;
use tree_sitter::{Node, Parser};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Source text and JVM language identifier used to derive structure features.
pub struct LanguageStructureRequest {
    pub source: String,
    /// LSP language identifier such as `java`, `kotlin`, `scala`, or `groovy`.
    pub language: String,
}

/// JVM-family languages that share the portable editor structure contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JvmLanguage {
    Java,
    Kotlin,
    Scala,
    Groovy,
}

impl JvmLanguage {
    /// Maps an LSP language identifier to the recognized JVM family member.
    pub(crate) fn from_language_id(value: &str) -> Option<Self> {
        match value {
            "java" => Some(Self::Java),
            "kotlin" => Some(Self::Kotlin),
            "scala" => Some(Self::Scala),
            "groovy" => Some(Self::Groovy),
            _ => None,
        }
    }

    fn grammar(self) -> Option<tree_sitter::Language> {
        match self {
            Self::Kotlin => Some(tree_sitter_kotlin_ng::LANGUAGE.into()),
            Self::Scala => Some(tree_sitter_scala::LANGUAGE.into()),
            Self::Groovy => Some(tree_sitter_groovy::LANGUAGE.into()),
            Self::Java => None,
        }
    }
}

/// Computes fold regions and syntax highlights for one JVM-family document.
pub(crate) fn language_structure(
    request: LanguageStructureRequest,
) -> Result<LanguageStructureResponse, CoreError> {
    let language = JvmLanguage::from_language_id(&request.language).ok_or_else(|| {
        CoreError::new(
            ErrorCode::InvalidRequest,
            format!("Unsupported JVM language identifier: {}", request.language),
        )
    })?;
    Ok(LanguageStructureResponse {
        fold_regions: fold_regions(&request.source, language),
        syntax_highlights: syntax_highlights(&request.source, language),
    })
}

/// Classifies JVM-family source into sorted, non-overlapping UTF-16 token ranges.
pub(crate) fn syntax_highlights(
    source: &str,
    language: JvmLanguage,
) -> Vec<JavaSyntaxHighlightResponse> {
    let Some(grammar) = language.grammar() else {
        return super::java_syntax::syntax_highlights(source);
    };
    let mut parser = Parser::new();
    if parser.set_language(&grammar).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    let utf16_offsets = utf16_offsets(source);
    let mut symbols = JvmSymbols::default();
    collect_symbols(tree.root_node(), source.as_bytes(), language, &mut symbols);
    collect_highlights(
        tree.root_node(),
        source,
        &utf16_offsets,
        language,
        &symbols,
        &mut values,
    );
    values.sort_by_key(|value| (value.utf16_start, value.utf16_length));
    values.dedup_by(|left, right| {
        left.utf16_start == right.utf16_start && left.utf16_length == right.utf16_length
    });
    values
}

/// Computes foldable regions for one JVM-family source document.
pub(crate) fn fold_regions(source: &str, language: JvmLanguage) -> Vec<JavaFoldRegionResponse> {
    match language {
        JvmLanguage::Java => super::java::fold_regions(source),
        JvmLanguage::Kotlin | JvmLanguage::Scala | JvmLanguage::Groovy => jvm_fold_regions(source),
    }
}

#[derive(Default)]
struct JvmSymbols<'a> {
    parameters: HashSet<&'a str>,
    fields: HashSet<&'a str>,
    constants: HashSet<&'a str>,
}

fn collect_highlights(
    node: Node<'_>,
    source: &str,
    utf16_offsets: &[usize],
    language: JvmLanguage,
    symbols: &JvmSymbols<'_>,
    values: &mut Vec<JavaSyntaxHighlightResponse>,
) {
    if let Some(role) = whole_node_role(node, source, language) {
        push_highlight(node, role, utf16_offsets, values);
        return;
    }
    if node.child_count() == 0 {
        if let Some(role) = leaf_role(node, source.as_bytes(), language, symbols) {
            push_highlight(node, role, utf16_offsets, values);
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_highlights(child, source, utf16_offsets, language, symbols, values);
    }
}

fn whole_node_role(node: Node<'_>, source: &str, language: JvmLanguage) -> Option<&'static str> {
    let kind = node.kind();
    match language {
        JvmLanguage::Kotlin => {
            if matches!(kind, "line_comment") {
                Some("comment")
            } else if kind == "block_comment" {
                Some(
                    if node
                        .utf8_text(source.as_bytes())
                        .is_ok_and(|text| text.starts_with("/**"))
                    {
                        "documentationComment"
                    } else {
                        "comment"
                    },
                )
            } else if matches!(kind, "string_literal" | "character_literal") {
                Some("string")
            } else if matches!(
                kind,
                "number_literal"
                    | "hex_literal"
                    | "bin_literal"
                    | "long_literal"
                    | "unsigned_literal"
                    | "real_literal"
            ) {
                Some("number")
            } else {
                None
            }
        }
        JvmLanguage::Scala => {
            if matches!(kind, "line_comment") {
                Some("comment")
            } else if kind == "block_comment" {
                Some(
                    if node
                        .utf8_text(source.as_bytes())
                        .is_ok_and(|text| text.starts_with("/**"))
                    {
                        "documentationComment"
                    } else {
                        "comment"
                    },
                )
            } else if matches!(
                kind,
                "string" | "interpolated_string" | "char_literal" | "character_literal"
            ) {
                Some("string")
            } else if matches!(
                kind,
                "integer_literal" | "floating_point_literal" | "hex_literal"
            ) {
                Some("number")
            } else {
                None
            }
        }
        JvmLanguage::Groovy => {
            if matches!(kind, "line_comment") {
                Some("comment")
            } else if kind == "block_comment" {
                Some(
                    if node
                        .utf8_text(source.as_bytes())
                        .is_ok_and(|text| text.starts_with("/**"))
                    {
                        "documentationComment"
                    } else {
                        "comment"
                    },
                )
            } else if matches!(kind, "string_literal") {
                Some("string")
            } else if matches!(
                kind,
                "decimal_integer_literal"
                    | "hex_integer_literal"
                    | "octal_integer_literal"
                    | "binary_integer_literal"
                    | "decimal_floating_point_literal"
                    | "hex_floating_point_literal"
            ) {
                Some("number")
            } else if matches!(
                kind,
                "boolean_type" | "integral_type" | "floating_point_type" | "void_type"
            ) {
                Some("keyword")
            } else {
                None
            }
        }
        JvmLanguage::Java => None,
    }
}

fn leaf_role(
    node: Node<'_>,
    source: &[u8],
    language: JvmLanguage,
    symbols: &JvmSymbols<'_>,
) -> Option<&'static str> {
    match language {
        JvmLanguage::Java => None,
        JvmLanguage::Kotlin => kotlin_leaf_role(node, source, symbols),
        JvmLanguage::Scala => scala_leaf_role(node, source, symbols),
        JvmLanguage::Groovy => groovy_leaf_role(node, source, symbols),
    }
}

fn kotlin_leaf_role(
    node: Node<'_>,
    source: &[u8],
    symbols: &JvmSymbols<'_>,
) -> Option<&'static str> {
    let kind = node.kind();
    if matches!(kind, "true" | "false") {
        return Some("boolean");
    }
    if kind == "null" {
        return Some("null");
    }
    if kind == "identifier" {
        return Some(kotlin_identifier_role(node, source, symbols));
    }
    if is_keyword(kind, &KOTLIN_KEYWORDS) {
        return Some("keyword");
    }
    if node.is_named() {
        return None;
    }
    Some(if is_operator(kind) {
        "operator"
    } else {
        "punctuation"
    })
}

fn kotlin_identifier_role(node: Node<'_>, source: &[u8], symbols: &JvmSymbols<'_>) -> &'static str {
    let Some(parent) = node.parent() else {
        return "variable";
    };
    match parent.kind() {
        "class_declaration" | "object_declaration" => return "type",
        "function_declaration" => return "functionDeclaration",
        "call_expression" => return "functionCall",
        "parameter" | "lambda_parameter" => return "parameter",
        // A primary-constructor parameter with `val`/`var` declares a property.
        "class_parameter" => {
            return if class_parameter_is_property(parent) {
                "field"
            } else {
                "parameter"
            };
        }
        _ => {}
    }
    if has_ancestor(node, "annotation") {
        return "annotation";
    }
    if has_ancestor(node, "user_type") {
        return "type";
    }
    if has_ancestor(node, "type_parameter") {
        return "typeParameter";
    }
    symbol_role(node, source, symbols)
}

fn scala_leaf_role(
    node: Node<'_>,
    source: &[u8],
    symbols: &JvmSymbols<'_>,
) -> Option<&'static str> {
    let kind = node.kind();
    if kind == "operator_identifier" {
        return Some("operator");
    }
    if matches!(kind, "true" | "false") || kind == "boolean_literal" {
        return Some("boolean");
    }
    if kind == "null" || kind == "null_literal" {
        return Some("null");
    }
    if kind == "type_identifier" {
        if has_ancestor(node, "annotation") {
            return Some("annotation");
        }
        if has_ancestor(node, "type_parameter") {
            return Some("typeParameter");
        }
        return Some("type");
    }
    if kind == "identifier" {
        let Some(parent) = node.parent() else {
            return Some("variable");
        };
        return Some(match parent.kind() {
            "class_definition" | "trait_definition" | "object_definition" | "enum_definition" => {
                "type"
            }
            "function_definition" | "function_declaration" => "functionDeclaration",
            "call_expression" => "functionCall",
            "parameter" | "class_parameter" => "parameter",
            _ => scala_identifier_role(node, source, symbols),
        });
    }
    if is_keyword(kind, &SCALA_KEYWORDS) {
        return Some("keyword");
    }
    if node.is_named() {
        return None;
    }
    Some(if is_operator(kind) {
        "operator"
    } else {
        "punctuation"
    })
}

fn scala_identifier_role(node: Node<'_>, source: &[u8], symbols: &JvmSymbols<'_>) -> &'static str {
    if has_ancestor(node, "annotation") {
        return "annotation";
    }
    if has_ancestor(node, "type_parameter") {
        return "typeParameter";
    }
    symbol_role(node, source, symbols)
}

fn groovy_leaf_role(
    node: Node<'_>,
    source: &[u8],
    symbols: &JvmSymbols<'_>,
) -> Option<&'static str> {
    let kind = node.kind();
    if matches!(kind, "true" | "false") {
        return Some("boolean");
    }
    if kind == "null_literal" {
        return Some("null");
    }
    if matches!(kind, "identifier" | "type_identifier") {
        if has_ancestor(node, "marker_annotation") || has_ancestor(node, "annotation") {
            return Some("annotation");
        }
    }
    if kind == "identifier" {
        return Some(groovy_identifier_role(node, source, symbols));
    }
    if kind == "type_identifier" {
        return Some("type");
    }
    if is_keyword(kind, &GROOVY_KEYWORDS) {
        return Some("keyword");
    }
    if node.is_named() {
        return None;
    }
    Some(if is_operator(kind) {
        "operator"
    } else {
        "punctuation"
    })
}

fn groovy_identifier_role(node: Node<'_>, source: &[u8], symbols: &JvmSymbols<'_>) -> &'static str {
    let Some(parent) = node.parent() else {
        return "variable";
    };
    match parent.kind() {
        "method_declaration" | "constructor_declaration" => return "functionDeclaration",
        "method_invocation" => {
            if field_is(parent, "name", node) {
                return "functionCall";
            }
        }
        "formal_parameter" | "spread_parameter" => return "parameter",
        "class_declaration" | "interface_declaration" | "enum_declaration" => return "type",
        "field_access" => {
            if field_is(parent, "field", node) {
                return "field";
            }
        }
        _ => {}
    }
    symbol_role(node, source, symbols)
}

/// Resolves an identifier through the collected declaration sets, then falls
/// back to the same uppercase type heuristic the Java classifier uses.
fn symbol_role(node: Node<'_>, source: &[u8], symbols: &JvmSymbols<'_>) -> &'static str {
    let Ok(name) = node.utf8_text(source) else {
        return "variable";
    };
    if symbols.parameters.contains(name) {
        return "parameter";
    }
    if symbols.constants.contains(name) {
        return "constant";
    }
    if symbols.fields.contains(name) {
        return "field";
    }
    if name
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
    {
        return "type";
    }
    "variable"
}

fn collect_symbols<'a>(
    node: Node<'_>,
    source: &'a [u8],
    language: JvmLanguage,
    symbols: &mut JvmSymbols<'a>,
) {
    match language {
        JvmLanguage::Kotlin => collect_kotlin_symbols(node, source, symbols),
        JvmLanguage::Scala => collect_scala_symbols(node, source, symbols),
        JvmLanguage::Groovy => collect_groovy_symbols(node, source, symbols),
        JvmLanguage::Java => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_symbols(child, source, language, symbols);
    }
}

fn collect_kotlin_symbols<'a>(node: Node<'_>, source: &'a [u8], symbols: &mut JvmSymbols<'a>) {
    match node.kind() {
        "parameter" | "lambda_parameter" => {
            if let Some(name) = child_identifier(node, source) {
                symbols.parameters.insert(name);
            }
        }
        "class_parameter" => {
            if let Some(name) = child_identifier(node, source) {
                if class_parameter_is_property(node) {
                    symbols.fields.insert(name);
                } else {
                    symbols.parameters.insert(name);
                }
            }
        }
        "property_declaration" => {
            let mut body_cursor = node.walk();
            let declaration = node
                .children(&mut body_cursor)
                .find(|child| child.kind() == "variable_declaration");
            let Some(declaration) = declaration else {
                return;
            };
            let Some(name) = child_identifier(declaration, source) else {
                return;
            };
            let mut modifier_cursor = node.walk();
            let is_constant = node
                .children(&mut modifier_cursor)
                .find(|child| child.kind() == "modifiers")
                .and_then(|modifiers| modifiers.utf8_text(source).ok())
                .is_some_and(|text| has_modifier(text, "const"));
            if is_constant {
                symbols.constants.insert(name);
            } else if node
                .parent()
                .is_some_and(|parent| parent.kind() == "class_body")
            {
                symbols.fields.insert(name);
            }
        }
        _ => {}
    }
}

fn collect_scala_symbols<'a>(node: Node<'_>, source: &'a [u8], symbols: &mut JvmSymbols<'a>) {
    let kind = node.kind();
    if kind != "val_definition" && kind != "var_definition" {
        return;
    }
    let Some(name) = child_identifier(node, source) else {
        return;
    };
    let is_val = kind == "val_definition";
    // Class-level definitions sit directly in a template body; anything nested
    // in a function block is a local binding.
    let in_template = node
        .parent()
        .is_some_and(|parent| parent.kind() == "template_body");
    let text = node.utf8_text(source).unwrap_or_default();
    if is_val && (has_modifier(text, "final") || is_screaming_case(name)) {
        symbols.constants.insert(name);
    } else if in_template {
        symbols.fields.insert(name);
    }
}

fn collect_groovy_symbols<'a>(node: Node<'_>, source: &'a [u8], symbols: &mut JvmSymbols<'a>) {
    match node.kind() {
        "formal_parameter" | "spread_parameter" => {
            if let Some(name) = child_identifier(node, source) {
                symbols.parameters.insert(name);
            }
        }
        "field_declaration" => {
            let mut declarator_cursor = node.walk();
            let declarator = node
                .children(&mut declarator_cursor)
                .find(|child| child.kind() == "variable_declarator");
            let Some(declarator) = declarator else {
                return;
            };
            let Some(name) = child_identifier(declarator, source) else {
                return;
            };
            let text = node.utf8_text(source).unwrap_or_default();
            if has_modifier(text, "static") && has_modifier(text, "final") {
                symbols.constants.insert(name);
            } else {
                symbols.fields.insert(name);
            }
        }
        _ => {}
    }
}

/// Detects a Kotlin primary-constructor `val`/`var` parameter, which declares
/// a property rather than a plain constructor argument.
fn class_parameter_is_property(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|child| matches!(child.kind(), "val" | "var"));
    found
}

/// Finds the declaration-name identifier of a definition node, preferring the
/// grammar's `name` field and falling back to the first plain identifier.
fn child_identifier<'a>(node: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    if let Some(name) = node
        .child_by_field_name("name")
        .and_then(|value| value.utf8_text(source).ok())
        .filter(|name| !name.is_empty())
    {
        return Some(name);
    }
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|child| child.kind() == "identifier");
    found
        .and_then(|value| value.utf8_text(source).ok())
        .filter(|name| !name.is_empty())
}

fn is_screaming_case(name: &str) -> bool {
    name.chars().any(|character| character.is_ascii_uppercase())
        && !name.chars().any(|character| character.is_ascii_lowercase())
}

fn utf16_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0; source.len() + 1];
    let mut utf16_offset = 0;
    for (byte_offset, character) in source.char_indices() {
        offsets[byte_offset] = utf16_offset;
        utf16_offset += character.len_utf16();
        offsets[byte_offset + character.len_utf8()] = utf16_offset;
    }
    offsets
}

fn push_highlight(
    node: Node<'_>,
    role: &str,
    utf16_offsets: &[usize],
    values: &mut Vec<JavaSyntaxHighlightResponse>,
) {
    let Some(&utf16_start) = utf16_offsets.get(node.start_byte()) else {
        return;
    };
    let Some(&utf16_end) = utf16_offsets.get(node.end_byte()) else {
        return;
    };
    let utf16_length = utf16_end.saturating_sub(utf16_start);
    if utf16_length == 0 {
        return;
    }
    values.push(JavaSyntaxHighlightResponse {
        utf16_start,
        utf16_length,
        role: role.to_string(),
    });
}

fn field_is(parent: Node<'_>, name: &str, node: Node<'_>) -> bool {
    parent
        .child_by_field_name(name)
        .is_some_and(|value| value.id() == node.id())
}

fn has_ancestor(mut node: Node<'_>, kind: &str) -> bool {
    while let Some(parent) = node.parent() {
        if parent.kind() == kind {
            return true;
        }
        node = parent;
    }
    false
}

fn has_modifier(text: &str, expected: &str) -> bool {
    text.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|word| word == expected)
}

fn is_keyword(kind: &str, keywords: &[&str]) -> bool {
    keywords.contains(&kind)
}

const KOTLIN_KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "break",
    "by",
    "catch",
    "class",
    "companion",
    "const",
    "constructor",
    "continue",
    "crossinline",
    "data",
    "do",
    "dynamic",
    "else",
    "enum",
    "external",
    "false",
    "field",
    "final",
    "finally",
    "for",
    "fun",
    "get",
    "if",
    "import",
    "in",
    "init",
    "infix",
    "interface",
    "internal",
    "is",
    "it",
    "lazy",
    "lateinit",
    "noinline",
    "null",
    "object",
    "open",
    "operator",
    "out",
    "override",
    "package",
    "param",
    "private",
    "property",
    "protected",
    "public",
    "reified",
    "return",
    "sealed",
    "set",
    "setparam",
    "super",
    "suspend",
    "tailrec",
    "this",
    "throw",
    "true",
    "try",
    "typealias",
    "typeof",
    "val",
    "var",
    "vararg",
    "when",
    "where",
    "while",
];

const SCALA_KEYWORDS: &[&str] = &[
    "abstract",
    "case",
    "catch",
    "class",
    "def",
    "derives",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "final",
    "finally",
    "for",
    "forSome",
    "given",
    "if",
    "implicit",
    "import",
    "infix",
    "inline",
    "instanceof",
    "lazy",
    "macro",
    "match",
    "new",
    "null",
    "object",
    "opaque",
    "open",
    "override",
    "package",
    "private",
    "protected",
    "returns",
    "sealed",
    "super",
    "then",
    "this",
    "throw",
    "throws",
    "trait",
    "transparent",
    "true",
    "try",
    "type",
    "val",
    "var",
    "while",
    "with",
    "yield",
];

const GROOVY_KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "assert",
    "boolean_type",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "def",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "false",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "null_literal",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "trait",
    "transient",
    "true",
    "try",
    "void",
    "volatile",
    "while",
];

fn is_operator(kind: &str) -> bool {
    matches!(
        kind,
        "=" | ">"
            | "<"
            | "!"
            | "~"
            | "?"
            | ":"
            | "->"
            | "=>"
            | "<-"
            | "?:"
            | "?."
            | "!!"
            | "=="
            | ">="
            | "<="
            | "!="
            | "==="
            | "!=="
            | "&&"
            | "||"
            | "++"
            | "--"
            | "+"
            | "-"
            | "*"
            | "/"
            | "&"
            | "|"
            | "^"
            | "%"
            | "<<"
            | ">>"
            | ">>>"
            | "+="
            | "-="
            | "*="
            | "/="
            | "&="
            | "|="
            | "^="
            | "%="
            | "<<="
            | ">>="
            | ">>>="
            | "::"
            | ".."
            | "..<"
    )
}

#[derive(Default)]
struct FoldScan {
    regions: Vec<JavaFoldRegionResponse>,
    stack: Vec<(usize, String)>,
    state: FoldState,
    triple_quote: u8,
}

#[derive(Default, PartialEq)]
enum FoldState {
    #[default]
    Code,
    String,
    Character,
    TripleString,
    LineComment,
    BlockComment,
}

fn jvm_fold_regions(source: &str) -> Vec<JavaFoldRegionResponse> {
    let mut regions = import_regions(source);
    regions.extend(comment_regions(source));
    regions.extend(brace_regions(source));
    regions.sort_by(|left, right| {
        left.start_line
            .cmp(&right.start_line)
            .then_with(|| right.end_line.cmp(&left.end_line))
    });
    regions
}

/// Folds contiguous top-of-file import blocks; JVM-family import lines do not
/// require a trailing semicolon.
fn import_regions(source: &str) -> Vec<JavaFoldRegionResponse> {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut import_lines = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("import ") || trimmed.starts_with("import\t") {
            import_lines.push(index);
        }
    }
    let (Some(first), Some(last)) = (import_lines.first(), import_lines.last()) else {
        return Vec::new();
    };
    if import_lines.len() < 2 {
        return Vec::new();
    }
    let first = *first;
    let last = *last;
    let hidden_start = line_end_offset(source, &lines, first);
    let hidden_end = line_end_offset(source, &lines, last);
    vec![JavaFoldRegionResponse {
        kind: "imports".to_string(),
        start_line: first,
        end_line: last,
        hidden_start,
        hidden_length: hidden_end.saturating_sub(hidden_start),
    }]
}

fn comment_regions(source: &str) -> Vec<JavaFoldRegionResponse> {
    let Ok(expression) = Regex::new(r"/\*[\s\S]*?\*/") else {
        return Vec::new();
    };
    let lines = source.split('\n').collect::<Vec<_>>();
    expression
        .find_iter(source)
        .filter_map(|matched| {
            let start_line = line_number(source, matched.start());
            let end_line = line_number(source, matched.end());
            (end_line > start_line).then(|| JavaFoldRegionResponse {
                kind: "comment".to_string(),
                start_line,
                end_line,
                hidden_start: line_end_offset(source, &lines, start_line),
                hidden_length: line_end_offset(source, &lines, end_line)
                    .saturating_sub(line_end_offset(source, &lines, start_line)),
            })
        })
        .collect()
}

fn brace_regions(source: &str) -> Vec<JavaFoldRegionResponse> {
    let lines = source.split('\n').collect::<Vec<_>>();
    let bytes = source.as_bytes();
    let mut scan = FoldScan::default();
    let mut index = 0;
    while index < bytes.len() {
        let character = bytes[index];
        let next = bytes.get(index + 1).copied().unwrap_or_default();
        let next2 = bytes.get(index + 2).copied().unwrap_or_default();
        match scan.state {
            FoldState::Code => match character {
                b'"' | b'\'' => {
                    if character == next && next == next2 {
                        scan.state = FoldState::TripleString;
                        scan.triple_quote = character;
                        index += 2;
                    } else if character == b'"' {
                        scan.state = FoldState::String;
                    } else {
                        scan.state = FoldState::Character;
                    }
                }
                b'/' if next == b'/' => {
                    scan.state = FoldState::LineComment;
                    index += 1;
                }
                b'/' if next == b'*' => {
                    scan.state = FoldState::BlockComment;
                    index += 1;
                }
                b'{' => {
                    let start = line_start(source, index);
                    scan.stack.push((index, source[start..index].to_string()));
                }
                b'}' => {
                    if let Some((opening, prefix)) = scan.stack.pop() {
                        let start_line = line_number(source, opening);
                        let end_line = line_number(source, index);
                        let hidden_start = line_end_offset(source, &lines, start_line);
                        let hidden_end = line_start(source, index);
                        if end_line > start_line && hidden_end > hidden_start {
                            scan.regions.push(JavaFoldRegionResponse {
                                kind: classify(&prefix),
                                start_line,
                                end_line,
                                hidden_start,
                                hidden_length: hidden_end.saturating_sub(hidden_start),
                            });
                        }
                    }
                }
                _ => {}
            },
            FoldState::String | FoldState::Character => {
                if character == b'\\' {
                    index += 1;
                } else if (scan.state == FoldState::String && character == b'"')
                    || (scan.state == FoldState::Character && character == b'\'')
                {
                    scan.state = FoldState::Code;
                }
            }
            FoldState::TripleString => {
                if character == b'\\' {
                    index += 1;
                } else if character == scan.triple_quote
                    && next == scan.triple_quote
                    && next2 == scan.triple_quote
                {
                    scan.state = FoldState::Code;
                    index += 2;
                }
            }
            FoldState::LineComment => {
                if character == b'\n' {
                    scan.state = FoldState::Code;
                }
            }
            FoldState::BlockComment => {
                if character == b'*' && next == b'/' {
                    scan.state = FoldState::Code;
                    index += 1;
                }
            }
        }
        index += 1;
    }
    scan.regions
}

fn classify(prefix: &str) -> String {
    if Regex::new(r"\b(class|interface|enum|record|object|trait|struct|protocol|extension|actor)\b")
        .expect("static JVM type expression is valid")
        .is_match(prefix)
    {
        "type".to_string()
    } else if prefix.contains('(') && prefix.contains(')') {
        "method".to_string()
    } else {
        "block".to_string()
    }
}

fn line_number(source: &str, byte: usize) -> usize {
    source[..byte.min(source.len())]
        .bytes()
        .filter(|value| *value == b'\n')
        .count()
}

fn line_start(source: &str, byte: usize) -> usize {
    source[..byte.min(source.len())]
        .rfind('\n')
        .map(|value| value + 1)
        .unwrap_or(0)
}

/// End offset (exclusive) of the newline for the zero-based `line` index.
fn line_end_offset(source: &str, lines: &[&str], line: usize) -> usize {
    let mut offset = 0;
    for (index, content) in lines.iter().enumerate() {
        if index == line {
            return offset + content.len();
        }
        offset += content.len() + 1;
    }
    source.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_at(
        source: &str,
        values: &[JavaSyntaxHighlightResponse],
        needle: &str,
    ) -> Option<String> {
        let utf16: Vec<u16> = source.encode_utf16().collect();
        values
            .iter()
            .find(|value| {
                utf16[value.utf16_start..value.utf16_start + value.utf16_length]
                    .iter()
                    .copied()
                    .eq(needle.encode_utf16())
            })
            .map(|value| value.role.clone())
    }

    #[test]
    fn classifies_kotlin_declarations_properties_and_strings() {
        let source = concat!(
            "/** Docs */\n",
            "@Deprecated\n",
            "data class User(val name: String) : Base() {\n",
            "    companion object {\n",
            "        const val MAX = 100\n",
            "    }\n",
            "    fun greet(prefix: String): String {\n",
            "        val local = \"$prefix $name ${age + MAX}\"\n",
            "        return local\n",
            "    }\n",
            "}\n"
        );
        let values = syntax_highlights(source, JvmLanguage::Kotlin);
        assert_eq!(
            token_at(source, &values, "/** Docs */").as_deref(),
            Some("documentationComment")
        );
        assert_eq!(
            token_at(source, &values, "Deprecated").as_deref(),
            Some("annotation")
        );
        assert_eq!(token_at(source, &values, "User").as_deref(), Some("type"));
        assert_eq!(token_at(source, &values, "Base").as_deref(), Some("type"));
        assert_eq!(
            token_at(source, &values, "MAX").as_deref(),
            Some("constant")
        );
        assert_eq!(
            token_at(source, &values, "greet").as_deref(),
            Some("functionDeclaration")
        );
        assert_eq!(
            token_at(source, &values, "prefix").as_deref(),
            Some("parameter")
        );
        assert_eq!(token_at(source, &values, "name").as_deref(), Some("field"));
        assert_eq!(
            token_at(source, &values, "local").as_deref(),
            Some("variable")
        );
        assert_eq!(token_at(source, &values, "100").as_deref(), Some("number"));
        assert_eq!(
            token_at(source, &values, "\"$prefix $name ${age + MAX}\"").as_deref(),
            Some("string")
        );
        assert_eq!(
            token_at(source, &values, "data").as_deref(),
            Some("keyword")
        );
    }

    #[test]
    fn classifies_scala_declarations_values_and_interpolation() {
        let source = concat!(
            "/** Docs */\n",
            "@Deprecated\n",
            "class User(val name: String) extends Base {\n",
            "  val MAX = 100\n",
            "  var count = 0\n",
            "  def greet(prefix: String): String = {\n",
            "    val local = s\"$prefix $MAX\"\n",
            "    local\n",
            "  }\n",
            "}\n",
            "trait Repo { def find(id: Long): Option[User] }\n",
            "object Factory { val instance = new User(\"a\", 1) }\n"
        );
        let values = syntax_highlights(source, JvmLanguage::Scala);
        assert_eq!(token_at(source, &values, "User").as_deref(), Some("type"));
        assert_eq!(token_at(source, &values, "Base").as_deref(), Some("type"));
        assert_eq!(
            token_at(source, &values, "Deprecated").as_deref(),
            Some("annotation")
        );
        assert_eq!(
            token_at(source, &values, "greet").as_deref(),
            Some("functionDeclaration")
        );
        assert_eq!(
            token_at(source, &values, "find").as_deref(),
            Some("functionDeclaration")
        );
        assert_eq!(
            token_at(source, &values, "Factory").as_deref(),
            Some("type")
        );
        assert_eq!(
            token_at(source, &values, "prefix").as_deref(),
            Some("parameter")
        );
        assert_eq!(
            token_at(source, &values, "MAX").as_deref(),
            Some("constant")
        );
        assert_eq!(token_at(source, &values, "count").as_deref(), Some("field"));
        assert_eq!(
            token_at(source, &values, "instance").as_deref(),
            Some("field")
        );
        assert_eq!(
            token_at(source, &values, "local").as_deref(),
            Some("variable")
        );
        assert_eq!(token_at(source, &values, "Option").as_deref(), Some("type"));
        assert_eq!(
            token_at(source, &values, "\"$prefix $MAX\"").as_deref(),
            Some("string")
        );
        assert_eq!(token_at(source, &values, "def").as_deref(), Some("keyword"));
    }

    #[test]
    fn classifies_groovy_declarations_fields_and_closures() {
        let source = concat!(
            "/** Docs */\n",
            "@Canonical\n",
            "class User {\n",
            "    String name\n",
            "    static final MAX = 100\n",
            "    String greet(String prefix) {\n",
            "        def local = [1, 2, 3]\n",
            "        return \"$prefix $name\"\n",
            "    }\n",
            "}\n",
            "def closure = { id -> println(\"v:\" + id) }\n"
        );
        let values = syntax_highlights(source, JvmLanguage::Groovy);
        assert_eq!(token_at(source, &values, "User").as_deref(), Some("type"));
        assert_eq!(
            token_at(source, &values, "Canonical").as_deref(),
            Some("annotation")
        );
        assert_eq!(
            token_at(source, &values, "greet").as_deref(),
            Some("functionDeclaration")
        );
        assert_eq!(
            token_at(source, &values, "prefix").as_deref(),
            Some("parameter")
        );
        assert_eq!(token_at(source, &values, "name").as_deref(), Some("field"));
        assert_eq!(
            token_at(source, &values, "local").as_deref(),
            Some("variable")
        );
        assert_eq!(
            token_at(source, &values, "\"$prefix $name\"").as_deref(),
            Some("string")
        );
        assert_eq!(token_at(source, &values, "def").as_deref(), Some("keyword"));
        assert_eq!(
            token_at(source, &values, "println").as_deref(),
            Some("functionCall")
        );
    }

    #[test]
    fn folds_jvm_imports_braces_and_comments_without_template_braces() {
        let source = concat!(
            "import kotlin.math.PI\n",
            "import kotlin.math.E\n",
            "\n",
            "class Demo {\n",
            "    fun text(): String {\n",
            "        return \"\"\"a { b } ${x}\"\"\"\n",
            "    }\n",
            "}\n"
        );
        let regions = fold_regions(source, JvmLanguage::Kotlin);
        assert!(regions.iter().any(|region| region.kind == "imports"
            && region.start_line == 0
            && region.end_line == 1));
        assert!(regions
            .iter()
            .any(|region| region.kind == "type" && region.start_line == 3));
        assert!(regions
            .iter()
            .any(|region| region.kind == "method" && region.start_line == 4));
        // Braces inside the triple-quoted template must not create fold regions.
        assert_eq!(
            regions
                .iter()
                .filter(|region| region.kind == "block")
                .count(),
            0
        );

        let commented = "class Demo {\n    /* multi\n    line */\n}\n";
        let regions = fold_regions(commented, JvmLanguage::Groovy);
        assert!(regions
            .iter()
            .any(|region| region.kind == "comment" && region.start_line == 1));
    }

    #[test]
    fn reports_document_relative_utf16_offsets_after_non_ascii_text() {
        let source = "class Demo { val text = \"😀\"; val count = 1 }";
        let values = syntax_highlights(source, JvmLanguage::Kotlin);
        let count_start = source.find("count").expect("count should exist");
        let expected = source[..count_start].encode_utf16().count();
        assert!(values.iter().any(|value| {
            value.utf16_start == expected && value.utf16_length == 5 && value.role == "field"
        }));
    }

    #[test]
    fn language_structure_delegates_java_and_rejects_unknown_languages() {
        let java = language_structure(LanguageStructureRequest {
            source: "class Demo {\n    void run() {\n    }\n}\n".to_string(),
            language: "java".to_string(),
        })
        .expect("java should be recognized");
        assert!(java
            .syntax_highlights
            .iter()
            .any(|value| value.role == "type"));
        assert!(java.fold_regions.iter().any(|region| region.kind == "type"));

        let kotlin = language_structure(LanguageStructureRequest {
            source: "class Demo\n".to_string(),
            language: "kotlin".to_string(),
        })
        .expect("kotlin should be recognized");
        assert!(!kotlin.syntax_highlights.is_empty());

        let error = language_structure(LanguageStructureRequest {
            source: "x".to_string(),
            language: "cobol".to_string(),
        })
        .expect_err("unknown languages must fail");
        assert!(matches!(error.code, ErrorCode::InvalidRequest));
    }
}
