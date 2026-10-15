#![cfg(feature = "narrative")]

use vesper3d::viewer::narrative::{self, Registry, ValueType};

fn registry() -> Registry {
    Registry {
        queries: [
            ("cells".into(), ValueType::Int),
            ("open".into(), ValueType::Bool),
        ]
        .into(),
        commands: [("power".into(), ValueType::Bool)].into(),
    }
}

const SCENE: &str = "poe 1\nscene relay\nnode offer\nspeaker keeper\ntext One cell. One bridge.\nchoice give consequential -> thanks | Power the bridge\nwhen cells >= 1\ndo power true\nendchoice\nchoice leave expressive -> end | I need a minute.\nendchoice\nendnode\nnode thanks\nspeaker keeper\ntext There. Listen to it hum.\nnext end\nendnode\nendscene\n";

#[test]
fn readable_content_keeps_directive_locations_and_choice_kind() {
    let content = narrative::parse_validated("relay.poe", SCENE, &registry()).unwrap();
    let choice = &content.scenes[0].nodes[0].choices[0];
    assert_eq!(choice.effects[0].source.line, 8);
    assert_eq!(choice.effects[0].source.node, "relay.offer.give");
    assert_eq!(choice.conditions[0].source.line, 7);
    assert_eq!(content.scenes[0].nodes[1].source.line, 13);
    assert!(narrative::lint(&content).is_empty());
}

#[test]
fn malformed_sources_name_file_node_and_actual_line() {
    for (input, line, message) in [
        (SCENE.replace("power true", "power 1"), 8, "command"),
        (SCENE.replace("cells >= 1", "cells >= true"), 7, "query"),
        (SCENE.replace("cells >= 1", "cells >= []"), 7, "JSON"),
        (SCENE.replace("next end", "next nowhere"), 13, "destination"),
        (
            SCENE.replace("text One cell. One bridge.", "text {missing}"),
            5,
            "interpolation",
        ),
        (
            SCENE.replace("next end", "next end\nnext end"),
            17,
            "duplicate",
        ),
    ] {
        let errors = narrative::parse_validated("broken.poe", &input, &registry()).unwrap_err();
        let error = errors
            .iter()
            .find(|e| e.message.contains(message))
            .unwrap_or_else(|| panic!("{errors:?}"));
        assert_eq!(error.source.file, "broken.poe");
        assert_eq!(error.source.line, line, "{error}");
        assert!(error.source.node.starts_with("relay."));
    }
}

#[test]
fn unsupported_syntax_and_versions_do_not_execute() {
    for input in [
        "poe 2",
        "poe 1\nscript shell anything",
        "poe 1\nresponse missing",
        "poe 1\nscene x\nnode x\nspeaker a\ntext b\nendnode\nendscene",
    ] {
        assert!(
            narrative::parse_validated("bad.poe", input, &registry()).is_err(),
            "{input}"
        );
    }
    assert!(narrative::parse_validated("empty.poe", "poe 1", &Registry::default()).is_ok());
}

#[test]
fn reconvergence_and_input_yielding_loops_are_valid() {
    let source = SCENE.replace("next end", "next offer");
    assert!(narrative::parse_validated("loop.poe", &source, &registry()).is_ok());
}

#[test]
fn bounds_and_unknown_bindings_are_hard_errors() {
    assert!(narrative::parse("large.poe", &" ".repeat(narrative::MAX_SOURCE_BYTES + 1)).is_err());
    assert!(narrative::parse_validated("relay.poe", SCENE, &Registry::default()).is_err());
    let source = SCENE.replace("next end", "");
    assert!(
        narrative::parse_validated("relay.poe", &source, &registry())
            .unwrap_err()
            .iter()
            .any(|d| d.message.contains("explicit"))
    );
}
