//! Public API snapshot guard for strata-core-next.

#![deny(unsafe_code)]

use std::{fs, path::Path};

const EXPECTED_PUBLIC_API: &str = include_str!("snapshots/public_api.txt");

#[test]
fn public_api_matches_snapshot() {
    let actual = current_public_api_snapshot();

    assert_eq!(
        actual,
        EXPECTED_PUBLIC_API.trim_end(),
        "strata-core-next public API changed; update the boundary document and snapshot together"
    );
}

fn current_public_api_snapshot() -> String {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = crate_root.join("src");
    let mut lines = vec![
        "# strata-core-next public API snapshot".to_owned(),
        "# This file is checked by tests/public_api_snapshot.rs.".to_owned(),
        "crate strata_core_next".to_owned(),
    ];

    let modules = collect_lib_exports(&src.join("lib.rs"), &mut lines);
    for module in modules {
        collect_module_api(&module, &src.join(format!("{module}.rs")), &mut lines);
    }

    lines.join("\n")
}

fn collect_lib_exports(path: &Path, lines: &mut Vec<String>) -> Vec<String> {
    lines.push(String::new());
    lines.push("module lib".to_owned());

    let mut modules = Vec::new();
    let source = read_source(path);
    for raw_line in source.lines() {
        let line = raw_line.trim();
        if let Some(module) = module_name_from_decl(line) {
            modules.push(module);
        }
        if line.starts_with("pub ") {
            lines.push(trim_semicolon(line));
        }
    }
    modules
}

fn collect_module_api(module: &str, path: &Path, lines: &mut Vec<String>) {
    lines.push(String::new());
    lines.push(format!("module {module}"));

    let source = read_source(path);
    let public_types = collect_public_types(&source, lines);
    collect_public_impls(&source, &public_types, lines);
}

fn collect_public_types(source: &str, lines: &mut Vec<String>) -> Vec<String> {
    let source_lines = source.lines().collect::<Vec<_>>();
    let mut index = 0;
    let mut pending_attrs = Vec::new();
    let mut public_types = Vec::new();

    while index < source_lines.len() {
        let line = source_lines[index].trim();

        if let Some((attr, next_index)) = public_type_attr(&source_lines, index) {
            pending_attrs.push(attr);
            index = next_index;
            continue;
        }

        if line.starts_with("pub struct ") {
            lines.append(&mut pending_attrs);
            if let Some(type_name) = public_type_name(line, "pub struct ") {
                public_types.push(type_name);
            }
            index = collect_public_struct(&source_lines, index, lines);
            continue;
        } else if line.starts_with("pub enum ") {
            lines.append(&mut pending_attrs);
            if let Some(type_name) = public_type_name(line, "pub enum ") {
                public_types.push(type_name);
            }
            index = collect_public_enum(&source_lines, index, lines);
            continue;
        } else if line.starts_with("pub ") {
            lines.append(&mut pending_attrs);
            lines.push(trim_semicolon(line));
        } else if !line.starts_with("#[") && !line.starts_with("///") && !line.is_empty() {
            pending_attrs.clear();
        }

        index = skip_source_item(&source_lines, index);
    }

    public_types
}

fn collect_public_impls(source: &str, public_types: &[String], lines: &mut Vec<String>) {
    let source_lines = source.lines().collect::<Vec<_>>();
    let mut index = 0;

    while index < source_lines.len() {
        let line = source_lines[index].trim();
        if let Some(header) = public_impl_header(line, public_types) {
            lines.push(header);
            let mut depth = brace_delta(line);
            let mut pending_attrs = Vec::new();
            index += 1;

            while index < source_lines.len() && depth > 0 {
                let impl_line = source_lines[index].trim();
                if impl_line.starts_with("#[") {
                    pending_attrs.push(format!("  {impl_line}"));
                } else if impl_line.starts_with("pub ") || impl_line.starts_with("type ") {
                    lines.append(&mut pending_attrs);
                    lines.push(format!("  {}", normalize_impl_item(impl_line)));
                } else if !impl_line.starts_with("///") && !impl_line.is_empty() {
                    pending_attrs.clear();
                }

                depth += brace_delta(impl_line);
                index += 1;
            }
            continue;
        }

        index += 1;
    }
}

fn collect_public_struct(source_lines: &[&str], index: usize, lines: &mut Vec<String>) -> usize {
    let line = source_lines[index].trim();

    if line.ends_with(';') || !line.contains('{') {
        lines.push(line.to_owned());
        return index + 1;
    }

    let Some((prefix, suffix)) = line.split_once('{') else {
        lines.push(line.to_owned());
        return index + 1;
    };

    lines.push(format!("{} {{", prefix.trim_end()));

    if let Some((body, _)) = suffix.rsplit_once('}') {
        collect_struct_fields(body.lines().map(str::trim), lines);
        lines.push("}".to_owned());
        return index + 1;
    }

    let mut depth = brace_delta(line);
    let mut next_index = index + 1;
    while next_index < source_lines.len() {
        let field_line = source_lines[next_index].trim();
        depth += brace_delta(field_line);

        if depth == 0 && field_line == "}" {
            lines.push("}".to_owned());
            return next_index + 1;
        }

        collect_struct_fields(std::iter::once(field_line), lines);
        next_index += 1;
    }

    lines.push("}".to_owned());
    next_index
}

fn collect_public_enum(source_lines: &[&str], index: usize, lines: &mut Vec<String>) -> usize {
    lines.push(source_lines[index].trim().to_owned());

    let mut next_index = index + 1;
    while next_index < source_lines.len() {
        let enum_line = source_lines[next_index].trim();
        if enum_line == "}" {
            lines.push("}".to_owned());
            return next_index + 1;
        }
        if is_enum_api_line(enum_line) {
            lines.push(format!("  {enum_line}"));
        }
        next_index += 1;
    }

    next_index
}

fn collect_struct_fields<'a>(fields: impl Iterator<Item = &'a str>, lines: &mut Vec<String>) {
    for field in fields {
        if is_struct_field_api_line(field) {
            if field.starts_with("pub ") {
                lines.push(format!("  {field}"));
            } else {
                lines.push(format!("  private {field}"));
            }
        }
    }
}

fn public_type_attr(source_lines: &[&str], index: usize) -> Option<(String, usize)> {
    let line = source_lines[index].trim();

    if line.starts_with("#[derive(") {
        let mut content = String::new();
        let mut next_index = index;
        loop {
            let mut current = source_lines[next_index].trim();
            if let Some(stripped) = current.strip_prefix("#[derive(") {
                current = stripped;
            }
            if let Some(stripped) = current.strip_suffix(")]") {
                current = stripped;
            }
            if !current.is_empty() {
                if !content.is_empty() {
                    content.push(' ');
                }
                content.push_str(current);
            }
            if source_lines[next_index].trim().ends_with(")]") {
                break;
            }
            next_index += 1;
        }

        return Some((
            format!("#[derive({})]", normalize_commas(&content)),
            next_index + 1,
        ));
    }

    if line.starts_with("#[") {
        return Some((line.to_owned(), index + 1));
    }

    None
}

fn module_name_from_decl(line: &str) -> Option<String> {
    let module = line
        .strip_prefix("mod ")
        .or_else(|| line.strip_prefix("pub mod "))?;
    let module = module.strip_suffix(';')?;
    Some(module.trim().to_owned())
}

fn public_type_name(line: &str, prefix: &str) -> Option<String> {
    let name = line
        .strip_prefix(prefix)?
        .split(|char: char| !(char.is_ascii_alphanumeric() || char == '_'))
        .next()?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

fn public_impl_header(line: &str, public_types: &[String]) -> Option<String> {
    if !line.starts_with("impl") {
        return None;
    }

    for public_type in public_types {
        if impl_header_mentions_type(line, public_type) {
            return Some(normalize_impl_header(line));
        }
    }

    None
}

fn impl_header_mentions_type(line: &str, public_type: &str) -> bool {
    let header = normalize_impl_header(line);
    let Some(impl_body) = header.strip_prefix("impl") else {
        return false;
    };
    let impl_body = impl_body.trim_start();

    if let Some((_, target)) = impl_body.rsplit_once(" for ") {
        return starts_with_type_name(target.trim(), public_type);
    }

    impl_body
        .split_whitespace()
        .any(|part| starts_with_type_name(part, public_type))
}

fn starts_with_type_name(candidate: &str, public_type: &str) -> bool {
    candidate == public_type || candidate.starts_with(&format!("{public_type}<"))
}

fn normalize_impl_header(line: &str) -> String {
    line.trim_end_matches(" {}")
        .trim_end_matches(" {")
        .to_owned()
}

fn normalize_impl_item(line: &str) -> String {
    line.trim_end_matches(" {")
        .trim_end_matches(" {}")
        .to_owned()
}

fn is_struct_field_api_line(line: &str) -> bool {
    !line.is_empty() && line != "}" && !line.starts_with("///") && !line.starts_with("#[")
}

fn is_enum_api_line(line: &str) -> bool {
    !line.is_empty()
        && !line.starts_with("///")
        && !line.starts_with("#[")
        && !line.starts_with("pub ")
}

fn normalize_commas(text: &str) -> String {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn brace_delta(line: &str) -> isize {
    line.chars().fold(0, |depth, char| match char {
        '{' => depth + 1,
        '}' => depth - 1,
        _ => depth,
    })
}

fn skip_source_item(source_lines: &[&str], index: usize) -> usize {
    let mut depth = brace_delta(source_lines[index].trim());
    let mut next_index = index + 1;

    while depth > 0 && next_index < source_lines.len() {
        depth += brace_delta(source_lines[next_index].trim());
        next_index += 1;
    }

    next_index
}

fn trim_semicolon(line: &str) -> String {
    line.trim_end_matches(';').to_owned()
}

fn read_source(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read source file {}: {error}", path.display()))
}
