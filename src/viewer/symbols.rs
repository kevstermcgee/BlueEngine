//! High-performance on-demand Rust source navigator for BlueEngine (`be2-tools src`).
//!
//! Indexes engine Rust sources directly without external dependencies or background daemons.
//! Allows AI agents and developers to explore architecture, symbols, signatures, references,
//! and dependencies in milliseconds without dumping whole files into context.
//!
//! Subcommands:
//! - `map`: all modules with line counts, public symbols, and doc summaries
//! - `find <query>`: searches symbols by name/keyword with signatures and doc comments
//! - `outline <file>`: outlines symbols within a specific file
//! - `show <symbol>`: extracts the exact definition lines of a symbol
//! - `refs <symbol>`: finds references to a symbol across the codebase
//! - `deps [module]`: analyzes module dependency graph
//! - `coverage`: reports public items lacking doc comments

use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Symbol {
    pub file: String,
    pub line: usize,
    pub end_line: usize,
    pub kind: String,
    pub name: String,
    pub container: Option<String>,
    pub is_public: bool,
    pub signature: String,
    pub doc: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleSummary {
    pub name: String,
    pub file: String,
    pub lines: usize,
    pub public_symbols: usize,
    pub total_symbols: usize,
    pub doc: String,
}

pub struct SourceIndex {
    pub root: PathBuf,
    pub files: Vec<(String, Vec<String>)>, // rel_path, lines
    pub symbols: Vec<Symbol>,
}

impl SourceIndex {
    pub fn scan(root: &Path) -> Result<Self> {
        let mut files = Vec::new();
        let src_dir = root.join("src");
        if src_dir.is_dir() {
            collect_rs(&src_dir, root, &mut files)?;
        }
        let tools_dir = root.join("tools");
        if tools_dir.is_dir() {
            collect_rs(&tools_dir, root, &mut files)?;
        }
        let tests_dir = root.join("tests");
        if tests_dir.is_dir() {
            collect_rs(&tests_dir, root, &mut files)?;
        }

        let mut symbols = Vec::new();
        for (rel, lines) in &files {
            parse_symbols(rel, lines, &mut symbols);
        }

        Ok(Self {
            root: root.to_path_buf(),
            files,
            symbols,
        })
    }

    pub fn map(&self) -> Vec<ModuleSummary> {
        let mut modules = Vec::new();
        for (rel, lines) in &self.files {
            if !rel.starts_with("src") {
                continue;
            }
            let file_syms: Vec<_> = self.symbols.iter().filter(|s| s.file == *rel).collect();
            let pub_count = file_syms.iter().filter(|s| s.is_public).count();

            // Extract file doc comment (top lines with `//!` or `///`)
            let mut doc = String::new();
            for line in lines {
                let trimmed = line.trim();
                if trimmed.starts_with("//!") || trimmed.starts_with("///") {
                    let text = trimmed
                        .trim_start_matches("/")
                        .trim_start_matches("!")
                        .trim();
                    if !text.is_empty() {
                        doc = text.to_string();
                        break;
                    }
                } else if !trimmed.is_empty() {
                    break;
                }
            }

            modules.push(ModuleSummary {
                name: rel.replace('\\', "/"),
                file: rel.clone(),
                lines: lines.len(),
                public_symbols: pub_count,
                total_symbols: file_syms.len(),
                doc,
            });
        }
        modules.sort_by(|a, b| a.name.cmp(&b.name));
        modules
    }

    pub fn find(&self, query: &str) -> Vec<Symbol> {
        let q = query.to_lowercase();
        let words: Vec<&str> = q.split_whitespace().collect();

        let mut matches = Vec::new();
        for sym in &self.symbols {
            let haystack =
                format!("{} {} {} {}", sym.name, sym.kind, sym.signature, sym.doc).to_lowercase();
            if words.iter().all(|w| haystack.contains(w)) {
                matches.push(sym.clone());
            }
        }
        matches.sort_by(|a, b| {
            let a_exact = a.name.to_lowercase() == q;
            let b_exact = b.name.to_lowercase() == q;
            b_exact.cmp(&a_exact).then_with(|| a.name.cmp(&b.name))
        });
        matches.truncate(25);
        matches
    }

    pub fn outline(&self, file_prefix: &str) -> Vec<Symbol> {
        let norm = file_prefix.replace('\\', "/");
        self.symbols
            .iter()
            .filter(|s| s.file.replace('\\', "/").contains(&norm))
            .cloned()
            .collect()
    }

    pub fn show(&self, symbol_name: &str) -> Result<String> {
        let sym = self
            .symbols
            .iter()
            .find(|s| {
                s.name == symbol_name
                    || format!("{}::{}", s.container.as_deref().unwrap_or(""), s.name)
                        == symbol_name
            })
            .ok_or_else(|| format!("Symbol '{symbol_name}' not found"))?;

        let file = self
            .files
            .iter()
            .find(|(rel, _)| rel == &sym.file)
            .ok_or_else(|| format!("File '{}' not found", sym.file))?;

        let start = sym.line.saturating_sub(1);
        let end = sym.end_line.min(file.1.len());

        let mut out = format!("// {}:{} ({})\n", sym.file, sym.line, sym.kind);
        if !sym.doc.is_empty() {
            out.push_str(&format!("/// {}\n", sym.doc));
        }
        for (idx, line) in file.1[start..end].iter().enumerate() {
            out.push_str(&format!("{:4} | {}\n", sym.line + idx, line));
        }
        Ok(out)
    }

    pub fn refs(&self, symbol_name: &str) -> BTreeMap<String, Vec<(usize, String)>> {
        let mut refs: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
        for (rel, lines) in &self.files {
            for (line_idx, line) in lines.iter().enumerate() {
                if line.contains(symbol_name) {
                    refs.entry(rel.clone())
                        .or_default()
                        .push((line_idx + 1, line.trim().to_string()));
                }
            }
        }
        refs
    }

    pub fn coverage(&self) -> Vec<Symbol> {
        self.symbols
            .iter()
            .filter(|s| s.is_public && s.doc.is_empty())
            .cloned()
            .collect()
    }
}

fn collect_rs(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<String>)>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, root, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            if let Ok(content) = std::fs::read_to_string(&path) {
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                out.push((rel, lines));
            }
        }
    }
    Ok(())
}

fn parse_symbols(rel: &str, lines: &[String], symbols: &mut Vec<Symbol>) {
    let mut current_container: Option<String> = None;
    let mut last_doc = String::new();

    let mut idx = 0;
    while idx < lines.len() {
        let line = lines[idx].trim();

        if line.starts_with("///") {
            let text = line.trim_start_matches('/').trim();
            if last_doc.is_empty() {
                last_doc = text.to_string();
            } else {
                last_doc.push(' ');
                last_doc.push_str(text);
            }
            idx += 1;
            continue;
        }

        if line.starts_with("//") {
            idx += 1;
            continue;
        }

        // Parse impl block header
        if line.starts_with("impl") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[1].trim_matches('{').trim();
                current_container = Some(name.to_string());
            }
        }

        let is_public = line.starts_with("pub ");
        let line_no_pub = if is_public {
            line.trim_start_matches("pub ").trim()
        } else {
            line
        };

        let kinds = ["fn", "struct", "enum", "trait", "type", "const", "static"];
        for &kind in &kinds {
            if line_no_pub.starts_with(kind) && line_no_pub[kind.len()..].starts_with(' ') {
                let rest = line_no_pub[kind.len()..].trim();
                let sym_name = rest
                    .split(|c: char| {
                        c == '('
                            || c == '<'
                            || c == ':'
                            || c == '{'
                            || c == ';'
                            || c.is_whitespace()
                    })
                    .next()
                    .unwrap_or("")
                    .trim();

                if !sym_name.is_empty() {
                    // Find symbol end line
                    let mut end_line = idx + 1;
                    if line.ends_with(';') {
                        end_line = idx + 1;
                    } else {
                        let mut brace_depth = 0;
                        let mut started = false;
                        for scan_i in idx..lines.len().min(idx + 100) {
                            let s = &lines[scan_i];
                            for ch in s.chars() {
                                if ch == '{' {
                                    brace_depth += 1;
                                    started = true;
                                } else if ch == '}' {
                                    brace_depth -= 1;
                                }
                            }
                            if started && brace_depth <= 0 {
                                end_line = scan_i + 1;
                                break;
                            }
                        }
                    }

                    symbols.push(Symbol {
                        file: rel.to_string(),
                        line: idx + 1,
                        end_line,
                        kind: kind.to_string(),
                        name: sym_name.to_string(),
                        container: current_container.clone(),
                        is_public,
                        signature: line.trim_end_matches('{').trim().to_string(),
                        doc: last_doc.clone(),
                    });
                }
                break;
            }
        }

        last_doc.clear();
        idx += 1;
    }
}
