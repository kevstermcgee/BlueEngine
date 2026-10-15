//! Optional bounded resource transactions for game-owned rules.
//!
//! An [`Inventory`] owns integer quantities, not recipes, machines, entity handles or presentation.
//! One transaction validates all costs against the original inventory, then all resulting quantities
//! against declared bounds, and commits together. Credits cannot finance costs in the same transaction.
//! Transactions do not impose conservation: game-owned recipes, sources and sinks declare why a
//! resource changes. A game owns escrow and must clear it only after a successful refund/output commit.
//!
//! Resources use persistent namespaced definition IDs, separate from runtime handles and labels.
//! Entries and failures are processed in lexicographic ID order, independently of input order.
//! [`MAX_RESOURCES`] and [`MAX_ENTRIES`] bound work; there is no clock, randomness, rendering or network
//! dependency. State serializes as schema version 1 with ordered maps and validates on deserialization.
//! Network visibility and replication budgets remain the game's responsibility.
//!
//! ```
//! use std::collections::BTreeMap;
//! use vesper3d::runtime::resources::Inventory;
//! let mut inventory = Inventory::new(20, BTreeMap::from([
//!     ("demo:ore".into(), 20), ("demo:plate".into(), 10),
//! ]))?;
//! inventory.transact(&[], &[("demo:ore".into(), 4)])?; // declared source
//! inventory.transact(&[("demo:ore".into(), 2)], &[("demo:plate".into(), 1)])?;
//! assert_eq!(inventory.quantity("demo:ore"), 2);
//! # Ok::<(), vesper3d::runtime::resources::TransactionError>(())
//! ```

use serde::{de, Deserialize, Deserializer, Serialize};
use std::{collections::BTreeMap, fmt};

/// Maximum number of declared resource definitions in one inventory.
pub const MAX_RESOURCES: usize = 64;
/// Maximum combined debit and credit input entries in one transaction, before aggregation.
pub const MAX_ENTRIES: usize = 128;
/// Maximum bytes in a persistent resource definition ID.
pub const MAX_ID_BYTES: usize = 128;
/// Version of the inventory's serialized representation, independent of a game's save version.
pub const SCHEMA_VERSION: u32 = 1;

/// An actionable refusal. Numeric fields apply when the corresponding bound caused the refusal.
/// A game can attach its authored source location using `resource`; errors do not mutate inventory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TransactionError {
    /// Stable diagnostic identifier, such as `RESOURCE-INSUFFICIENT` or `RESOURCE-CAPACITY`.
    pub code: &'static str,
    /// Persistent definition ID involved, if the refusal concerns one resource.
    pub resource: Option<String>,
    /// Quantity available before the transaction, when relevant.
    pub available: Option<u64>,
    /// Quantity requested, computed, or unsupported schema version, when relevant.
    pub required: Option<u64>,
    /// Declared resource, inventory, entry-count or schema bound, when relevant.
    pub capacity: Option<u64>,
    /// What failed and how the author/player can recover.
    pub message: String,
}

impl TransactionError {
    fn new(code: &'static str, resource: Option<&str>, message: impl Into<String>) -> Self {
        Self {
            code,
            resource: resource.map(str::to_owned),
            available: None,
            required: None,
            capacity: None,
            message: message.into(),
        }
    }

    fn bound(mut self, required: u64, capacity: u64) -> Self {
        self.required = Some(required);
        self.capacity = Some(capacity);
        self
    }
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for TransactionError {}

/// One touched resource's balance. A receipt describes this local authoritative commit only;
/// it is not a remote-delivery acknowledgement or an authorization to repeat the transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResourceChange {
    /// Persistent resource definition ID.
    pub resource: String,
    /// Quantity before this transaction.
    pub before: u64,
    /// Aggregated gross debit.
    pub debited: u64,
    /// Aggregated gross credit.
    pub credited: u64,
    /// Quantity after this transaction.
    pub after: u64,
}

/// Sorted bounded evidence of one committed transaction; not a persistent event log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TransactionReceipt {
    /// Touched resources in lexicographic ID order, at most [`MAX_RESOURCES`].
    pub balances: Vec<ResourceChange>,
    /// Sum of all stored resource units before commit.
    pub total_before: u64,
    /// Sum of all stored resource units after commit.
    pub total_after: u64,
}

/// A bounded inventory of declared resource definitions.
///
/// Capacities and counts are private so every mutation crosses the same validation boundary.
/// `schema_version`, `max_units`, `capacities` and `counts` serialize as plain data; use this exact
/// value inside the game's engine-owned [`super::Snapshot::State`] rather than writing save files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Inventory {
    schema_version: u32,
    max_units: u64,
    capacities: BTreeMap<String, u64>,
    counts: BTreeMap<String, u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryState {
    schema_version: u32,
    max_units: u64,
    #[serde(deserialize_with = "bounded_resource_map")]
    capacities: BTreeMap<String, u64>,
    #[serde(deserialize_with = "bounded_resource_map")]
    counts: BTreeMap<String, u64>,
}

// Bound resource-map growth and reject duplicate JSON keys before inserting into the map. Derived
// BTreeMap deserialization alone would silently replace an earlier duplicate definition.
fn bounded_resource_map<'de, D>(deserializer: D) -> Result<BTreeMap<String, u64>, D::Error>
where
    D: Deserializer<'de>,
{
    struct ResourceMap;
    impl<'de> de::Visitor<'de> for ResourceMap {
        type Value = BTreeMap<String, u64>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "at most {MAX_RESOURCES} distinct namespaced resource IDs"
            )
        }

        fn visit_map<A>(self, mut entries: A) -> Result<Self::Value, A::Error>
        where
            A: de::MapAccess<'de>,
        {
            let mut map = BTreeMap::new();
            while let Some(id) = entries.next_key::<String>()? {
                validate_id(&id).map_err(de::Error::custom)?;
                if map.len() >= MAX_RESOURCES {
                    return Err(de::Error::custom(
                        "RESOURCE-DEFINITIONS: inventory exceeds 64 resources",
                    ));
                }
                if map.contains_key(&id) {
                    return Err(de::Error::custom(format!(
                        "RESOURCE-DUPLICATE: resource '{id}' appears twice"
                    )));
                }
                map.insert(id, entries.next_value()?);
            }
            Ok(map)
        }
    }
    deserializer.deserialize_map(ResourceMap)
}

impl<'de> Deserialize<'de> for Inventory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let state = InventoryState::deserialize(deserializer)?;
        let inventory = Self {
            schema_version: state.schema_version,
            max_units: state.max_units,
            capacities: state.capacities,
            counts: state.counts,
        };
        inventory.validate().map_err(de::Error::custom)?;
        Ok(inventory)
    }
}

fn valid_id(id: &str) -> bool {
    let part = |value: &str| {
        !value.is_empty()
            && value
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-.".contains(&c))
    };
    id.len() <= MAX_ID_BYTES
        && id
            .split_once(':')
            .is_some_and(|(namespace, name)| part(namespace) && part(name))
}

fn validate_id(id: &str) -> Result<(), TransactionError> {
    // Invalid external IDs must not create unbounded copies in diagnostics.
    if id.len() > MAX_ID_BYTES {
        return Err(TransactionError::new(
            "RESOURCE-ID",
            None,
            format!(
                "resource ID has {} bytes; shorten it to at most {MAX_ID_BYTES} bytes",
                id.len()
            ),
        )
        .bound(id.len() as u64, MAX_ID_BYTES as u64));
    }
    if valid_id(id) {
        Ok(())
    } else {
        Err(TransactionError::new(
            "RESOURCE-ID",
            Some(id),
            format!("resource '{id}' must be namespace:name using lowercase ASCII letters, digits, _, - or .; maximum {MAX_ID_BYTES} bytes"),
        ))
    }
}

impl Inventory {
    /// Declare up to [`MAX_RESOURCES`] namespaced definitions, initially all zero.
    /// `max_units` bounds their combined count; capacities bound each resource separately.
    /// Zero capacities/budget are valid and deliberately refuse storing units.
    pub fn new(
        max_units: u64,
        capacities: BTreeMap<String, u64>,
    ) -> Result<Self, TransactionError> {
        Self::from_counts(max_units, capacities, BTreeMap::new())
    }

    /// Restore or prepare plain counts under explicit bounds. Missing declared counts become zero;
    /// unknown IDs, invalid IDs, over-capacity counts and arithmetic overflow are refused.
    /// A game's Snapshot restore must validate game-specific escrow/recipe invariants separately.
    pub fn from_counts(
        max_units: u64,
        capacities: BTreeMap<String, u64>,
        mut counts: BTreeMap<String, u64>,
    ) -> Result<Self, TransactionError> {
        // Reject caller-provided oversized inputs before adding any map entries.
        if capacities.len() > MAX_RESOURCES || counts.len() > MAX_RESOURCES {
            return Err(TransactionError::new(
                "RESOURCE-DEFINITIONS",
                None,
                format!("inventory declares too many resources; limit is {MAX_RESOURCES}"),
            )
            .bound(
                capacities.len().max(counts.len()) as u64,
                MAX_RESOURCES as u64,
            ));
        }
        if let Some(id) = counts.keys().find(|id| !capacities.contains_key(*id)) {
            validate_id(id)?;
            return Err(TransactionError::new(
                "RESOURCE-UNKNOWN", Some(id),
                format!("resource '{id}' is not declared in this inventory; correct the saved count or declare the resource"),
            ));
        }
        for id in capacities.keys() {
            validate_id(id)?;
            counts.entry(id.clone()).or_default();
        }
        let inventory = Self {
            schema_version: SCHEMA_VERSION,
            max_units,
            capacities,
            counts,
        };
        inventory.validate()?;
        Ok(inventory)
    }

    /// Validate serialized schema, definitions, quantities and total resource budget.
    pub fn validate(&self) -> Result<(), TransactionError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(TransactionError::new(
                "RESOURCE-SCHEMA", None,
                format!("inventory schema {} is unsupported; this runtime reads schema {SCHEMA_VERSION}", self.schema_version),
            ).bound(u64::from(self.schema_version), u64::from(SCHEMA_VERSION)));
        }
        if self.capacities.len() > MAX_RESOURCES || self.counts.len() > MAX_RESOURCES {
            return Err(TransactionError::new(
                "RESOURCE-DEFINITIONS",
                None,
                format!("inventory exceeds {MAX_RESOURCES} resource definitions"),
            )
            .bound(
                self.capacities.len().max(self.counts.len()) as u64,
                MAX_RESOURCES as u64,
            ));
        }
        for id in self.capacities.keys() {
            validate_id(id)?;
            if !self.counts.contains_key(id) {
                return Err(TransactionError::new(
                    "RESOURCE-MISSING", Some(id),
                    format!("inventory is missing the count for declared resource '{id}'; use from_counts to initialize omitted counts"),
                ));
            }
        }
        let mut total = 0u64;
        for (id, &quantity) in &self.counts {
            let &capacity = self.capacities.get(id).ok_or_else(|| self.unknown(id))?;
            if quantity > capacity {
                return Err(self.full(id, quantity, capacity));
            }
            total = total
                .checked_add(quantity)
                .ok_or_else(|| self.overflow(Some(id)))?;
        }
        if total > self.max_units {
            return Err(self.budget(total));
        }
        Ok(())
    }

    /// Stored quantity; zero for an unknown ID. Transactions still refuse unknown IDs.
    pub fn quantity(&self, resource: &str) -> u64 {
        self.counts.get(resource).copied().unwrap_or(0)
    }

    /// Sorted authoritative quantities. Callers cannot mutate them outside a transaction.
    pub fn counts(&self) -> &BTreeMap<String, u64> {
        &self.counts
    }

    /// Sorted declared resource capacities.
    pub fn capacities(&self) -> &BTreeMap<String, u64> {
        &self.capacities
    }

    /// Declared combined resource-unit budget.
    pub fn max_units(&self) -> u64 {
        self.max_units
    }

    /// Total stored units; construction, deserialization and transactions prevent overflow.
    pub fn total_units(&self) -> u64 {
        self.counts.values().sum()
    }

    /// Validate gross debits and final credits, then commit all touched quantities coherently.
    ///
    /// At most [`MAX_ENTRIES`] combined entries are accepted before aggregation/allocation.
    /// Duplicate entries aggregate using checked arithmetic; even zero entries require declared IDs.
    /// Costs must be available before credits. Capacity applies to the final state, so replacing a
    /// consumed resource can succeed in a full inventory. Every refusal preserves the entire value.
    pub fn transact(
        &mut self,
        debits: &[(String, u64)],
        credits: &[(String, u64)],
    ) -> Result<TransactionReceipt, TransactionError> {
        if debits.len() > MAX_ENTRIES || credits.len() > MAX_ENTRIES - debits.len() {
            return Err(TransactionError::new(
                "RESOURCE-ENTRIES", None,
                format!("transaction has too many input entries; combine or split intentional operations (limit {MAX_ENTRIES})"),
            ).bound(debits.len().saturating_add(credits.len()) as u64, MAX_ENTRIES as u64));
        }
        if let Some(id) = debits
            .iter()
            .chain(credits)
            .map(|(id, _)| id)
            .filter(|id| !valid_id(id))
            .min()
        {
            return Err(validate_id(id).expect_err("invalid resource ID"));
        }
        if let Some(id) = debits
            .iter()
            .chain(credits)
            .map(|(id, _)| id)
            .filter(|id| !self.capacities.contains_key(*id))
            .min()
        {
            return Err(self.unknown(id));
        }
        // At most 128 u64 entries fit in u128. Aggregate first, then report arithmetic refusals in
        // sorted definition order rather than whichever duplicate entry the caller happened to list.
        let mut changes: BTreeMap<&str, (u128, u128)> = BTreeMap::new();
        for (is_credit, entries) in [(false, debits), (true, credits)] {
            for (id, quantity) in entries {
                let entry = changes.entry(id.as_str()).or_default();
                let sum = if is_credit {
                    &mut entry.1
                } else {
                    &mut entry.0
                };
                *sum += u128::from(*quantity);
            }
        }
        let total_before = self.total_units();
        let mut debited_total = 0u64;
        let mut credited_total = 0u64;
        let mut balances = Vec::with_capacity(changes.len());
        for (id, (debited, credited)) in changes {
            let debited = u64::try_from(debited).map_err(|_| self.overflow(Some(id)))?;
            let credited = u64::try_from(credited).map_err(|_| self.overflow(Some(id)))?;
            let before = self.quantity(id);
            if debited > before {
                let mut error = TransactionError::new(
                    "RESOURCE-INSUFFICIENT", Some(id),
                    format!("resource '{id}' has {before} units but requires {debited}; credits in this transaction cannot pay its costs"),
                );
                error.available = Some(before);
                error.required = Some(debited);
                return Err(error);
            }
            let after = (before - debited)
                .checked_add(credited)
                .ok_or_else(|| self.overflow(Some(id)))?;
            let capacity = self.capacities[id];
            if after > capacity {
                return Err(self.full(id, after, capacity));
            }
            debited_total = debited_total
                .checked_add(debited)
                .ok_or_else(|| self.overflow(Some(id)))?;
            credited_total = credited_total
                .checked_add(credited)
                .ok_or_else(|| self.overflow(Some(id)))?;
            balances.push(ResourceChange {
                resource: id.to_owned(),
                before,
                debited,
                credited,
                after,
            });
        }
        let total_after = (total_before - debited_total)
            .checked_add(credited_total)
            .ok_or_else(|| self.overflow(None))?;
        if total_after > self.max_units {
            return Err(self.budget(total_after));
        }
        // All checks finish before any authoritative mutation; no external callbacks run here.
        for balance in &balances {
            *self
                .counts
                .get_mut(&balance.resource)
                .expect("declared resource count") = balance.after;
        }
        Ok(TransactionReceipt {
            balances,
            total_before,
            total_after,
        })
    }

    fn unknown(&self, id: &str) -> TransactionError {
        TransactionError::new(
            "RESOURCE-UNKNOWN", Some(id),
            format!("resource '{id}' is not declared in this inventory; add its persistent definition/capacity or correct the reference"),
        )
    }

    fn overflow(&self, id: Option<&str>) -> TransactionError {
        TransactionError::new(
            "RESOURCE-OVERFLOW",
            id,
            "resource arithmetic exceeds u64; reduce the declared quantities or transaction size",
        )
    }

    fn full(&self, id: &str, required: u64, capacity: u64) -> TransactionError {
        TransactionError::new(
            "RESOURCE-CAPACITY", Some(id),
            format!("resource '{id}' would hold {required} units but capacity is {capacity}; free space before retrying"),
        ).bound(required, capacity)
    }

    fn budget(&self, required: u64) -> TransactionError {
        TransactionError::new(
            "RESOURCE-BUDGET", None,
            format!("inventory would hold {required} total units but its budget is {}; free space before retrying", self.max_units),
        ).bound(required, self.max_units)
    }
}
