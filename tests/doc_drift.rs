//! Integration tests ensuring documentation and codebase never drift out of sync.

use std::path::Path;
use vesper3d::viewer::doc_drift::audit_documentation;

#[test]
fn test_documentation_drift_protection() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let report = audit_documentation(root).expect("audit_documentation failed");

    assert!(
        report.is_clean(),
        "Documentation drift detected:\n{}",
        report.explain()
    );

    assert!(
        report.scanned_markdown_files >= 30,
        "Expected at least 30 markdown files, found {}",
        report.scanned_markdown_files
    );
    assert!(
        report.checked_links >= 20,
        "Expected at least 20 local markdown links, found {}",
        report.checked_links
    );
    assert!(
        report.checked_json_blocks >= 5,
        "Expected at least 5 embedded JSON blocks, found {}",
        report.checked_json_blocks
    );
    assert!(
        report.checked_features >= 25,
        "Expected at least 25 features in FEATURES.json, found {}",
        report.checked_features
    );
    assert!(
        report.undocumented_commands.is_empty(),
        "All commands must be documented, found missing: {:?}",
        report.undocumented_commands
    );
}
