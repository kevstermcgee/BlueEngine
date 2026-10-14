//! External public-API resource transactions and a game-owned ability fixture.
use std::collections::BTreeMap;
use vesper3d::runtime::{
    resources::{Inventory, TransactionError, MAX_ENTRIES, MAX_RESOURCES},
    Simulation,
};

fn pair(id: &str, amount: u64) -> (String, u64) {
    (id.to_owned(), amount)
}

fn inventory() -> Inventory {
    Inventory::from_counts(
        10,
        BTreeMap::from([pair("test:ore", 8), pair("test:plate", 5)]),
        BTreeMap::from([pair("test:ore", 6), pair("test:plate", 2)]),
    )
    .unwrap()
}

fn refused_unchanged(
    inventory: &mut Inventory,
    debits: &[(String, u64)],
    credits: &[(String, u64)],
    code: &str,
) -> TransactionError {
    let before = inventory.clone();
    let bytes_before = serde_json::to_vec(inventory).unwrap();
    let error = inventory.transact(debits, credits).unwrap_err();
    assert_eq!(error.code, code, "{error}");
    assert_eq!(*inventory, before);
    assert_eq!(serde_json::to_vec(inventory).unwrap(), bytes_before);
    error
}

#[test]
fn coherent_recipe_commits_and_duplicate_entries_aggregate() {
    let mut inventory = inventory();
    let receipt = inventory
        .transact(
            &[pair("test:ore", 1), pair("test:ore", 2)],
            &[pair("test:plate", 1), pair("test:plate", 1)],
        )
        .unwrap();
    assert_eq!(inventory.quantity("test:ore"), 3);
    assert_eq!(inventory.quantity("test:plate"), 4);
    assert_eq!((receipt.total_before, receipt.total_after), (8, 7));
    assert_eq!(receipt.balances[0].resource, "test:ore");
    assert_eq!(receipt.balances[0].debited, 3);
    assert_eq!(receipt.balances[1].credited, 2);
    inventory.validate().unwrap();
}

#[test]
fn insufficient_cost_full_item_and_full_total_preserve_every_quantity() {
    let mut inventory = inventory();
    let error = refused_unchanged(
        &mut inventory,
        &[pair("test:ore", 7)],
        &[pair("test:plate", 1)],
        "RESOURCE-INSUFFICIENT",
    );
    assert_eq!(error.resource.as_deref(), Some("test:ore"));
    assert_eq!((error.available, error.required), (Some(6), Some(7)));
    let error = refused_unchanged(
        &mut inventory,
        &[pair("test:ore", 1)],
        &[pair("test:plate", 4)],
        "RESOURCE-CAPACITY",
    );
    assert_eq!((error.required, error.capacity), (Some(6), Some(5)));
    let error = refused_unchanged(
        &mut inventory,
        &[],
        &[pair("test:ore", 2), pair("test:plate", 1)],
        "RESOURCE-BUDGET",
    );
    assert_eq!((error.required, error.capacity), (Some(11), Some(10)));
}

#[test]
fn credits_cannot_finance_gross_costs_but_capacity_uses_final_state() {
    let mut inventory = inventory();
    refused_unchanged(
        &mut inventory,
        &[pair("test:ore", 7)],
        &[pair("test:ore", 7)],
        "RESOURCE-INSUFFICIENT",
    );
    inventory.transact(&[], &[pair("test:ore", 2)]).unwrap(); // inventory now full at 10 units
    inventory
        .transact(&[pair("test:ore", 2)], &[pair("test:plate", 2)])
        .unwrap();
    assert_eq!(inventory.total_units(), 10);
    assert_eq!(inventory.quantity("test:plate"), 4);
}

#[test]
fn undeclared_zero_entries_and_oversized_transactions_are_refused() {
    let mut inventory = inventory();
    let error = refused_unchanged(
        &mut inventory,
        &[pair("test:missing", 0)],
        &[],
        "RESOURCE-UNKNOWN",
    );
    assert!(error.to_string().contains("declare"));
    let excessive = vec![pair("test:ore", 0); MAX_ENTRIES + 1];
    refused_unchanged(&mut inventory, &excessive, &[], "RESOURCE-ENTRIES");
    let limit = vec![pair("test:ore", 0); MAX_ENTRIES];
    inventory.transact(&limit, &[]).unwrap();
    refused_unchanged(
        &mut inventory,
        &limit,
        &[pair("test:plate", 0)],
        "RESOURCE-ENTRIES",
    );
    let error = refused_unchanged(
        &mut inventory,
        &[("a:".repeat(1000), 1)],
        &[],
        "RESOURCE-ID",
    );
    assert!(
        error.message.len() < 200,
        "oversized IDs must not inflate diagnostics"
    );
    assert_eq!(error.resource, None);
}

#[test]
fn checked_arithmetic_refuses_aggregation_balance_and_total_overflow() {
    let mut inventory = Inventory::from_counts(
        u64::MAX,
        BTreeMap::from([pair("test:a", u64::MAX), pair("test:b", u64::MAX)]),
        BTreeMap::from([pair("test:a", 1)]),
    )
    .unwrap();
    refused_unchanged(
        &mut inventory,
        &[],
        &[pair("test:a", u64::MAX), pair("test:a", 1)],
        "RESOURCE-OVERFLOW",
    );
    refused_unchanged(
        &mut inventory,
        &[],
        &[pair("test:a", u64::MAX)],
        "RESOURCE-OVERFLOW",
    );
    inventory
        .transact(&[], &[pair("test:b", u64::MAX - 1)])
        .unwrap();
    refused_unchanged(
        &mut inventory,
        &[],
        &[pair("test:a", 1)],
        "RESOURCE-OVERFLOW",
    );
}

#[test]
fn errors_and_receipts_do_not_depend_on_entry_order() {
    let mut first = inventory();
    let mut second = first.clone();
    let a = vec![
        pair("test:plate", 1),
        pair("test:ore", 2),
        pair("test:ore", 1),
    ];
    let mut b = a.clone();
    b.reverse();
    assert_eq!(first.transact(&a, &[]), second.transact(&b, &[]));
    let a = vec![pair("test:z", 0), pair("test:missing", 1)];
    let mut b = a.clone();
    b.reverse();
    assert_eq!(first.transact(&a, &[]), second.transact(&b, &[]));
}

#[test]
fn serialized_inventory_is_stable_and_invalid_or_duplicate_state_is_refused() {
    let inventory = inventory();
    let bytes = serde_json::to_vec(&inventory).unwrap();
    let loaded: Inventory = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(loaded, inventory);
    assert_eq!(serde_json::to_vec(&loaded).unwrap(), bytes);
    let mut malformed = serde_json::to_value(&inventory).unwrap();
    for (path, value, code) in [
        ("schema_version", serde_json::json!(2), "RESOURCE-SCHEMA"),
        ("max_units", serde_json::json!(1), "RESOURCE-BUDGET"),
        (
            "counts",
            serde_json::json!({"test:ore": 20, "test:plate": 0}),
            "RESOURCE-CAPACITY",
        ),
        (
            "counts",
            serde_json::json!({"test:ore": 0}),
            "RESOURCE-MISSING",
        ),
        (
            "counts",
            serde_json::json!({"test:ore": 0, "test:plate": 0, "test:unknown": 0}),
            "RESOURCE-UNKNOWN",
        ),
    ] {
        malformed[path] = value;
        let error = serde_json::from_value::<Inventory>(malformed.clone()).unwrap_err();
        assert!(error.to_string().contains(code), "{error}");
        malformed = serde_json::to_value(&inventory).unwrap();
    }
    let duplicate = r#"{"schema_version":1,"max_units":5,"capacities":{"test:ore":5,"test:ore":4},"counts":{"test:ore":0}}"#;
    assert!(serde_json::from_str::<Inventory>(duplicate)
        .unwrap_err()
        .to_string()
        .contains("RESOURCE-DUPLICATE"));
}

#[test]
fn invalid_definitions_and_counts_are_refused_at_construction() {
    for id in [
        "ore",
        "workshop:",
        ":ore",
        "WorkShop:ore",
        "a:b:c",
        "a:hello/world",
    ] {
        assert_eq!(
            Inventory::new(1, BTreeMap::from([pair(id, 1)]))
                .unwrap_err()
                .code,
            "RESOURCE-ID"
        );
    }
    let too_many = (0..=MAX_RESOURCES)
        .map(|n| pair(&format!("test:item-{n}"), 1))
        .collect();
    assert_eq!(
        Inventory::new(1, too_many).unwrap_err().code,
        "RESOURCE-DEFINITIONS"
    );
    let unknown = Inventory::from_counts(
        1,
        BTreeMap::from([pair("test:a", 1)]),
        BTreeMap::from([pair("test:b", 0)]),
    );
    assert_eq!(unknown.unwrap_err().code, "RESOURCE-UNKNOWN");
    let empty = Inventory::new(0, BTreeMap::from([pair("test:a", 0)])).unwrap();
    assert_eq!(empty.total_units(), 0);
}

#[test]
fn exhaustive_small_transactions_match_gross_cost_and_final_bound_contract() {
    for before_a in 0..=3 {
        for before_b in 0..=3 {
            if before_a + before_b > 4 {
                continue;
            }
            for debit_a in 0..=4 {
                for debit_b in 0..=4 {
                    for credit_a in 0..=4 {
                        for credit_b in 0..=4 {
                            let mut inventory = Inventory::from_counts(
                                4,
                                BTreeMap::from([pair("test:a", 3), pair("test:b", 3)]),
                                BTreeMap::from([
                                    pair("test:a", before_a),
                                    pair("test:b", before_b),
                                ]),
                            )
                            .unwrap();
                            let before = inventory.clone();
                            let accepted = inventory.transact(
                                &[pair("test:a", debit_a), pair("test:b", debit_b)],
                                &[pair("test:a", credit_a), pair("test:b", credit_b)],
                            );
                            let expected = debit_a <= before_a
                                && debit_b <= before_b
                                && before_a - debit_a.min(before_a) + credit_a <= 3
                                && before_b - debit_b.min(before_b) + credit_b <= 3
                                && before_a + before_b
                                    - debit_a.min(before_a)
                                    - debit_b.min(before_b)
                                    + credit_a
                                    + credit_b
                                    <= 4;
                            assert_eq!(accepted.is_ok(), expected);
                            if accepted.is_ok() {
                                assert_eq!(
                                    inventory.quantity("test:a"),
                                    before_a - debit_a + credit_a
                                );
                                assert_eq!(
                                    inventory.quantity("test:b"),
                                    before_b - debit_b + credit_b
                                );
                            } else {
                                assert_eq!(inventory, before);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Default)]
enum AbilityInput {
    #[default]
    Wait,
    Aim,
    Cancel,
    Release,
}

/// A second tiny headless consumer: an ability reserves mana while aiming. Rules and timing belong
/// to this fixture, not Inventory. Released mana is an explicitly declared sink; a ward is a source.
struct Ability {
    inventory: Inventory,
    reserved: u64,
    cooldown: u32,
}
impl Ability {
    fn new() -> Self {
        Self {
            inventory: Inventory::from_counts(
                11,
                BTreeMap::from([pair("ability:mana", 10), pair("ability:ward", 1)]),
                BTreeMap::from([pair("ability:mana", 6)]),
            )
            .unwrap(),
            reserved: 0,
            cooldown: 0,
        }
    }
}
impl Simulation for Ability {
    type Input = AbilityInput;
    fn step(&mut self, input: &Self::Input) {
        self.cooldown = self.cooldown.saturating_sub(1);
        match input {
            AbilityInput::Aim if self.reserved == 0 && self.cooldown == 0 => {
                if self
                    .inventory
                    .transact(&[pair("ability:mana", 3)], &[])
                    .is_ok()
                {
                    self.reserved = 3;
                }
            }
            AbilityInput::Cancel if self.reserved > 0 => {
                if self
                    .inventory
                    .transact(&[], &[pair("ability:mana", self.reserved)])
                    .is_ok()
                {
                    self.reserved = 0;
                }
            }
            AbilityInput::Release
                if self.reserved > 0
                    && self
                        .inventory
                        .transact(&[], &[pair("ability:ward", 1)])
                        .is_ok() =>
            {
                self.reserved = 0;
                self.cooldown = 3;
            }
            _ => {}
        }
    }
    fn state_hash(&self) -> u64 {
        let mut h = vesper3d::runtime::StateHasher::new();
        h.u64(self.reserved).u32(self.cooldown);
        for (id, count) in self.inventory.counts() {
            h.str(id).u64(*count);
        }
        h.finish()
    }
}

#[test]
fn headless_ability_reuses_transactions_with_game_owned_cost_refund_and_cooldown() {
    let inputs = [
        AbilityInput::Aim,
        AbilityInput::Cancel,
        AbilityInput::Aim,
        AbilityInput::Release,
        AbilityInput::Aim,
        AbilityInput::Wait,
        AbilityInput::Aim,
        AbilityInput::Release,
        AbilityInput::Cancel,
    ];
    vesper3d::runtime::assert_deterministic(Ability::new, &inputs);
    let mut ability = Ability::new();
    ability.step(&AbilityInput::Aim);
    assert_eq!(
        (ability.inventory.quantity("ability:mana"), ability.reserved),
        (3, 3)
    );
    ability.step(&AbilityInput::Cancel);
    assert_eq!(
        (ability.inventory.quantity("ability:mana"), ability.reserved),
        (6, 0)
    );
    ability.step(&AbilityInput::Aim);
    ability.step(&AbilityInput::Release);
    assert_eq!(ability.inventory.quantity("ability:ward"), 1);
    ability.step(&AbilityInput::Aim);
    assert_eq!(ability.reserved, 0, "cooldown is authoritative game state");
    ability.step(&AbilityInput::Wait);
    ability.step(&AbilityInput::Aim);
    ability.step(&AbilityInput::Release);
    assert_eq!(
        ability.reserved, 3,
        "full ward destination keeps reserved mana intact"
    );
    ability.step(&AbilityInput::Cancel);
    assert_eq!(
        (ability.inventory.quantity("ability:mana"), ability.reserved),
        (3, 0)
    );
    assert_eq!(ability.inventory.quantity("ability:ward"), 1);
}
