//! `game-validate` names the record that is missing or wrong when an interactable's map records disagree.
use vesper3d::{
    math::V,
    scene::Track,
    viewer::{authoring::MapDocument, game::GameDocument, game_example},
};

fn fixture() -> (GameDocument, MapDocument) {
    game_example::documents().unwrap()
}
fn error_after(edit: impl FnOnce(&mut MapDocument)) -> String {
    let (document, mut map) = fixture();
    edit(&mut map);
    document
        .validate(&map)
        .expect_err("the edited map should be refused")
        .to_string()
}

#[test]
fn a_missing_record_is_named_and_the_others_are_shown_as_found() {
    let message = error_after(|m| m.scene.nodes.retain(|n| n.id != "button-a"));
    assert!(message.contains("Interactable 'button-a'"), "{message}");
    assert!(
        message.contains("scene node (MISSING)")
            && message.contains("collider (found)")
            && message.contains("entity (found)"),
        "{message}"
    );
    assert!(
        message.contains("add-interactable"),
        "the fix is named: {message}"
    );

    let message = error_after(|m| {
        m.colliders.remove("button-b");
    });
    assert!(
        message.contains("collider (MISSING)") && message.contains("scene node (found)"),
        "{message}"
    );

    let message = error_after(|m| m.entities.retain(|e| e.id != "button-c"));
    assert!(
        message.contains("'button-c'") && message.contains("entity (MISSING)"),
        "{message}"
    );
}

#[test]
fn disagreeing_bounds_say_which_pair_differs_and_by_how_much() {
    let message = error_after(|m| {
        m.colliders.get_mut("button-a").unwrap().max.2 += 0.05;
    });
    assert!(
        message.contains("the collider") && message.contains("does not match the node box"),
        "{message}"
    );
    assert!(
        message.contains("1.349"),
        "the offending value is shown: {message}"
    );

    let message = error_after(|m| {
        m.entities
            .iter_mut()
            .find(|e| e.id == "button-a")
            .unwrap()
            .bounds
            .min
            .0 -= 0.05;
    });
    assert!(
        message.contains("entity bounds") && message.contains("do not match the collider"),
        "{message}"
    );
}

#[test]
fn shape_rotation_and_material_problems_are_each_reported_for_what_they_are() {
    let message = error_after(|m| {
        m.scene
            .nodes
            .iter_mut()
            .find(|n| n.id == "button-a")
            .unwrap()
            .rot = Track::Fixed(V(0.0, 0.5, 0.0));
    });
    assert!(message.contains("'button-a' is rotated"), "{message}");
    let message = error_after(|m| {
        let material = m.scene.materials["edit-button-a"].clone();
        m.scene.materials.insert("prop-vase".into(), material);
        m.scene
            .nodes
            .iter_mut()
            .find(|n| n.id == "button-a")
            .unwrap()
            .material = "prop-vase".into();
    });
    assert!(
        message.contains("prop/decor material 'prop-vase'"),
        "{message}"
    );
}

#[test]
fn a_correct_game_is_still_accepted() {
    let (document, map) = fixture();
    document.validate(&map).unwrap();
}
