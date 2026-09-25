//! Strong automated documentation drift protection.
//!
//! Validates:
//! 1. All CLI commands in `COMMANDS` are documented in documentation markdown files.
//! 2. Command invocation snippets in documentation match valid registered command names and argument limits.
//! 3. Local relative markdown links (`[text](path)`) resolve to existing files on disk.
//! 4. Embedded JSON blocks (` ```json ... ``` `) parse strictly without syntax errors.
//! 5. Feature index entries in `tools/FEATURES.json` reference valid, existing files.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{viewer::capabilities::COMMANDS, Result};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationKind {
    BrokenLink,
    InvalidJsonBlock,
    UndocumentedCommand,
    InvalidCommandInvocation,
    MissingFeatureFile,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocDriftViolation {
    pub kind: ViolationKind,
    pub file: String,
    pub line: usize,
    pub message: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DocDriftReport {
    pub ok: bool,
    pub scanned_markdown_files: usize,
    pub checked_links: usize,
    pub checked_json_blocks: usize,
    pub checked_features: usize,
    pub undocumented_commands: Vec<String>,
    pub violations: Vec<DocDriftViolation>,
}

impl DocDriftReport {
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }

    pub fn explain(&self) -> String {
        if self.is_clean() {
            format!(
                "Documentation is clean: {} markdown files, {} links, {} JSON blocks, and {} features verified without drift.",
                self.scanned_markdown_files, self.checked_links, self.checked_json_blocks, self.checked_features
            )
        } else {
            let mut out = format!(
                "Documentation drift detected ({} violations):\n",
                self.violations.len()
            );
            for v in &self.violations {
                out.push_str(&format!(
                    "  - [{:?}] {}:{}: {}\n",
                    v.kind, v.file, v.line, v.message
                ));
            }
            out
        }
    }
}

/// Recursively find all `.md` files starting from `root`, skipping build/git directories.
pub fn collect_markdown_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_md_recursive(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_md_recursive(dir: &Path, acc: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.') || name_str == "target" || name_str == ".be2-work" {
            continue;
        }

        if path.is_dir() {
            collect_md_recursive(&path, acc)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            acc.push(path);
        }
    }
    Ok(())
}

/// Perform a complete documentation drift audit on the repository.
pub fn audit_documentation(root: &Path) -> Result<DocDriftReport> {
    let md_files = collect_markdown_files(root)?;
    let mut report = DocDriftReport {
        scanned_markdown_files: md_files.len(),
        ..Default::default()
    };

    let mut mentioned_commands = HashSet::new();

    // Map command name to (min_args, max_args)
    let command_map: std::collections::HashMap<&str, (usize, usize)> = COMMANDS
        .iter()
        .map(|(name, sig)| {
            let words: Vec<&str> = sig.split_whitespace().collect();
            let min_args = words.iter().filter(|w| !w.starts_with('[')).count();
            let max_args = words.len();
            (*name, (min_args, max_args))
        })
        .collect();

    // 1. Scan markdown files for links, JSON blocks, command invocations, and command coverage
    for md_path in &md_files {
        let bytes = std::fs::read(md_path)?;
        let content = String::from_utf8_lossy(&bytes);
        let rel_file = md_path
            .strip_prefix(root)
            .unwrap_or(md_path)
            .to_string_lossy()
            .replace('\\', "/");

        let lines: Vec<&str> = content.lines().collect();

        // Check command mentions across document
        for &cmd in command_map.keys() {
            if content.contains(cmd) {
                mentioned_commands.insert(cmd.to_string());
            }
        }

        // Line-by-line checks
        let mut in_code_block = false;
        let mut in_json_block = false;
        let mut json_block_start_line = 0;
        let mut json_block_buf = String::new();

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();

            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
            }

            // Track JSON code blocks
            if trimmed.starts_with("```json") {
                in_json_block = true;
                json_block_start_line = line_num;
                json_block_buf.clear();
                continue;
            } else if in_json_block && trimmed.starts_with("```") {
                in_json_block = false;
                report.checked_json_blocks += 1;
                if let Err(e) = serde_json::from_str::<serde_json::Value>(&json_block_buf) {
                    report.violations.push(DocDriftViolation {
                        kind: ViolationKind::InvalidJsonBlock,
                        file: rel_file.clone(),
                        line: json_block_start_line,
                        message: format!("Malformed embedded JSON code block: {e}"),
                    });
                }
                json_block_buf.clear();
                continue;
            } else if in_json_block {
                json_block_buf.push_str(line);
                json_block_buf.push('\n');
                continue;
            }

            // Check markdown local links: [text](target)
            let mut search_idx = 0;
            while let Some(start_bracket) = line[search_idx..].find('[') {
                let abs_bracket = search_idx + start_bracket;
                if let Some(end_bracket) = line[abs_bracket..].find(']') {
                    let abs_end_bracket = abs_bracket + end_bracket;
                    if line.as_bytes().get(abs_end_bracket + 1) == Some(&b'(') {
                        let link_start = abs_end_bracket + 2;
                        if let Some(link_end_rel) = line[link_start..].find(')') {
                            let link_end = link_start + link_end_rel;
                            let target = line[link_start..link_end].trim();
                            search_idx = link_end + 1;

                            // Skip web URLs, mailto, anchor-only links
                            if target.starts_with("http://")
                                || target.starts_with("https://")
                                || target.starts_with("mailto:")
                                || target.starts_with('#')
                                || target.is_empty()
                            {
                                continue;
                            }

                            report.checked_links += 1;
                            // Strip any anchor suffix #...
                            let clean_target = target.split('#').next().unwrap_or(target);
                            if clean_target.is_empty() {
                                continue;
                            }

                            let resolved = if let Some(parent) = md_path.parent() {
                                parent.join(clean_target)
                            } else {
                                PathBuf::from(clean_target)
                            };

                            if !resolved.exists() {
                                report.violations.push(DocDriftViolation {
                                    kind: ViolationKind::BrokenLink,
                                    file: rel_file.clone(),
                                    line: line_num,
                                    message: format!(
                                        "Broken local link '{target}' does not resolve on disk (checked {:?})",
                                        resolved
                                    ),
                                });
                            }
                            continue;
                        }
                    }
                }
                search_idx = abs_bracket + 1;
            }

            // Check CLI invocations: only inside `be2-tools ...` code spans or code blocks
            let has_code_span = line.contains("`be2-tools ");
            let is_code_line = in_code_block && trimmed.starts_with("be2-tools ");
            if has_code_span || is_code_line {
                let marker = if has_code_span {
                    "`be2-tools "
                } else {
                    "be2-tools "
                };
                if let Some(tool_call) = line.find(marker) {
                    let after = &line[tool_call + marker.len()..];
                    let snippet = after
                        .trim_start()
                        .split(['`', '"', '\'', '\n'])
                        .next()
                        .unwrap_or("");
                    let parts: Vec<&str> = snippet
                        .split_whitespace()
                        .map(|p| {
                            p.trim_matches(|c: char| {
                                c == ';' || c == ',' || c == '.' || c == ')' || c == '('
                            })
                        })
                        .collect();
                    if let Some(&subcmd) = parts.first() {
                        if !subcmd.is_empty()
                            && !subcmd.starts_with('<')
                            && !subcmd.starts_with('.')
                            && !subcmd.starts_with('-')
                        {
                            if let Some(&(min_args, max_args)) = command_map.get(subcmd) {
                                let arg_count = parts.len() - 1;
                                if !snippet.contains("...")
                                    && (arg_count < min_args || arg_count > max_args)
                                {
                                    report.violations.push(DocDriftViolation {
                                        kind: ViolationKind::InvalidCommandInvocation,
                                        file: rel_file.clone(),
                                        line: line_num,
                                        message: format!(
                                            "be2-tools {} invocation has {} arguments, expected {min_args}..={max_args}",
                                            subcmd, arg_count
                                        ),
                                    });
                                }
                            } else {
                                report.violations.push(DocDriftViolation {
                                    kind: ViolationKind::InvalidCommandInvocation,
                                    file: rel_file.clone(),
                                    line: line_num,
                                    message: format!("Unknown be2-tools command '{subcmd}' invoked in documentation"),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Validate Command Documentation Coverage
    for (name, _) in COMMANDS {
        if !mentioned_commands.contains(*name) {
            report.undocumented_commands.push(name.to_string());
            report.violations.push(DocDriftViolation {
                kind: ViolationKind::UndocumentedCommand,
                file: "tools/README.md".into(),
                line: 1,
                message: format!(
                    "Command '{name}' is declared in COMMANDS but not documented anywhere in docs/"
                ),
            });
        }
    }

    // 3. Validate tools/FEATURES.json integrity
    let features_file = root.join("tools").join("FEATURES.json");
    if features_file.exists() {
        let content = std::fs::read_to_string(&features_file)?;
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(features) = val.get("features").and_then(|f| f.as_object()) {
                for (feat_name, feat_val) in features {
                    report.checked_features += 1;
                    if let Some(files) = feat_val.get("files").and_then(|fl| fl.as_array()) {
                        for f in files {
                            if let Some(f_str) = f.as_str() {
                                let path = root.join(f_str);
                                if !path.exists() {
                                    report.violations.push(DocDriftViolation {
                                        kind: ViolationKind::MissingFeatureFile,
                                        file: "tools/FEATURES.json".into(),
                                        line: 1,
                                        message: format!(
                                            "Feature '{feat_name}' references file '{f_str}' which does not exist"
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    report.ok = report.is_clean();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_markdown_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let files = collect_markdown_files(root).unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.ends_with("README.md")));
    }
}
