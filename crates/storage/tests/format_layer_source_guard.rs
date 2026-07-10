//! Source guards for durable byte ownership at the L3 format boundary.

#![deny(unsafe_code)]

mod common;

use std::path::Path;

#[test]
fn lifecycle_and_service_do_not_reimplement_l3_snapshot_or_extension_codecs() {
    let root = common::crate_root();
    let mut files = Vec::new();
    for dir in ["src/lifecycle", "src/service"] {
        common::source_guard_helpers::collect_rs_files(&root.join(dir), &mut files);
    }

    let violations: Vec<_> = files
        .into_iter()
        .filter(|file| is_production_source(file))
        .flat_map(|file| l3_codec_marker_violations(&root, &file))
        .collect();

    assert!(
        violations.is_empty(),
        "durable snapshot-row and retained-history bytes must stay in src/format:\n{}",
        violations.join("\n")
    );
}

#[test]
fn l3_source_guard_catches_snapshot_row_payload_constants() {
    for line in [
        r#"const SNAPSHOT_ROWS_MAGIC: [u8; 4] = *b"STRR";"#,
        "const SNAPSHOT_ROWS_VERSION: u32 = 1;",
        "let payload = b\"STRR\";",
        "let row_count = u32::from_le_bytes(payload[8..12].try_into().unwrap());",
        "let row_len = u32::from_le_bytes(payload[cursor..cursor + 4].try_into().unwrap());",
    ] {
        assert!(
            forbidden_l3_codec_marker(line).is_some(),
            "guard should reject {line:?}"
        );
    }
}

#[test]
fn l3_source_guard_catches_retained_history_payload_parser_fragments() {
    for line in [
        "const RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN: usize = 24;",
        r#"let section = TableManifestExtensionSection::optional("storage.retained_history", true, payload);"#,
        r#"return Err(FormatError::InvalidValue { field: "retained_history_timestamp_flag" });"#,
        r#"return Err(FormatError::InvalidValue { field: "retained_history_reserved_bytes" });"#,
        "payload.extend_from_slice(&retained_version_floor.as_u64().to_le_bytes());",
        "payload.extend_from_slice(&retained_timestamp_floor.as_micros().to_le_bytes());",
    ] {
        assert!(
            forbidden_l3_codec_marker(line).is_some(),
            "guard should reject {line:?}"
        );
    }
}

#[test]
fn l3_source_guard_allows_calls_into_format_codecs() {
    for line in [
        "encode_snapshot_row_section(rows)",
        "decode_snapshot_row_payload(section.payload())",
        "encode_retained_history_extension_payload(self.to_payload())",
        "decode_retained_history_extension_payload(payload).map(Self::from_payload)",
    ] {
        assert!(
            forbidden_l3_codec_marker(line).is_none(),
            "guard should allow {line:?}"
        );
    }
}

#[test]
fn l3_source_guard_skips_cfg_test_items() {
    let source = r#"
fn production() {
    let payload = b"STRR";
}

#[cfg(test)]
fn fixture() {
    let payload = b"STRR";
}

#[cfg(test)]
// kept separate to prove comments between cfg and item are skipped
fn commented_fixture() {
    let payload = b"STRR";
}

#[cfg(test)]

fn spaced_fixture() {
    let payload = b"STRR";
}

#[cfg(test)]
fn multiline_fixture()
{
    let payload = b"STRR";
}

#[cfg(test)]
pub(crate) use test_support::SNAPSHOT_ROWS_MAGIC;

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    const RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN: usize = 24;
}
"#;

    let production = production_lines(source);
    assert!(
        production
            .iter()
            .any(|(_index, line)| line.contains("STRR")),
        "production marker should remain visible to the guard"
    );
    assert!(
        production
            .iter()
            .all(|(_index, line)| !line.contains("RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN")),
        "cfg-test module marker should be hidden from production scan"
    );
    assert!(
        production
            .iter()
            .all(|(_index, line)| !line.contains("SNAPSHOT_ROWS_MAGIC")),
        "cfg-test use marker should be hidden from production scan"
    );
    assert_eq!(
        production
            .iter()
            .filter(|(_index, line)| line.contains("STRR"))
            .count(),
        1,
        "cfg-test function markers should be hidden from production scan"
    );
}

fn l3_codec_marker_violations(root: &Path, file: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(file).expect("read source file");
    let relative = file.strip_prefix(root).unwrap_or(file).display();
    production_lines(&text)
        .into_iter()
        .filter_map(|(index, line)| {
            forbidden_l3_codec_marker(line)
                .map(|marker| format!("{relative}:{} contains `{marker}`", index + 1))
        })
        .collect()
}

fn forbidden_l3_codec_marker(line: &str) -> Option<&'static str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return None;
    }

    [
        "SNAPSHOT_ROWS_MAGIC",
        "SNAPSHOT_ROWS_VERSION",
        "SNAPSHOT_ROWS_HEADER_SIZE",
        "*b\"STRR\"",
        "b\"STRR\"",
        "u32::from_le_bytes(payload[8..12]",
        "u32::from_le_bytes(payload[cursor..cursor + 4]",
        "RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN",
        "\"storage.retained_history\"",
        "retained_history_timestamp_flag",
        "retained_history_reserved_bytes",
        "retained_version_floor.as_u64().to_le_bytes()",
        "retained_timestamp_floor.as_micros().to_le_bytes()",
    ]
    .into_iter()
    .find(|marker| line.contains(marker))
}

fn is_production_source(file: &Path) -> bool {
    !file.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name == "testkit" || name == "tests" || name == "test_support" || name.ends_with("_tests")
    }) && file.file_name().is_some_and(|file_name| {
        let file_name = file_name.to_string_lossy();
        file_name != "tests.rs"
            && file_name != "test_support.rs"
            && !file_name.ends_with("_tests.rs")
    })
}

fn production_lines(text: &str) -> Vec<(usize, &str)> {
    let mut lines = Vec::new();
    let mut cfg_test_pending = false;
    let mut cfg_test_waiting_for_body = false;
    let mut cfg_test_depth: Option<usize> = None;
    let mut brace_depth = 0usize;

    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if cfg_test_depth.is_none()
            && !cfg_test_waiting_for_body
            && trimmed.starts_with("#[cfg(test)]")
        {
            cfg_test_pending = true;
            continue;
        }

        if cfg_test_pending
            && (trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("#["))
        {
            continue;
        }

        if cfg_test_pending {
            let opens = count_open_braces(line);
            let closes = count_close_braces(line);
            if opens > closes {
                cfg_test_depth = Some(brace_depth + opens - closes);
                brace_depth = cfg_test_depth.unwrap_or(brace_depth);
            } else if opens == 0 && closes == 0 && !trimmed.ends_with(';') {
                cfg_test_waiting_for_body = true;
            }
            cfg_test_pending = false;
            continue;
        }

        if cfg_test_waiting_for_body {
            let opens = count_open_braces(line);
            let closes = count_close_braces(line);
            if opens > closes {
                cfg_test_depth = Some(brace_depth + opens - closes);
                brace_depth = cfg_test_depth.unwrap_or(brace_depth);
                cfg_test_waiting_for_body = false;
            } else if (opens == closes && opens > 0) || trimmed.ends_with(';') {
                cfg_test_waiting_for_body = false;
            }
            continue;
        }

        if cfg_test_depth.is_none() {
            lines.push((index, line));
        }

        let opens = count_open_braces(line);
        let closes = count_close_braces(line);
        brace_depth = brace_depth.saturating_add(opens).saturating_sub(closes);
        if let Some(test_depth) = cfg_test_depth {
            if brace_depth < test_depth {
                cfg_test_depth = None;
            }
        }
    }

    lines
}

fn count_open_braces(line: &str) -> usize {
    line.bytes().filter(|byte| *byte == b'{').count()
}

fn count_close_braces(line: &str) -> usize {
    line.bytes().filter(|byte| *byte == b'}').count()
}
