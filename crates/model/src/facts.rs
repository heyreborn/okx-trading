//! Strict version-one cross-process fact shapes. Exchange payloads must first
//! be parsed and validated by an adapter. This module does not trust, persist,
//! order, approve or submit any fact. See the [package README](../README.md).

use crate::identity::{
    AccountId, BindingId, ClientOrderId, EventId, ExchangeOrderId, FeedId, FillId, OkxInstrumentId,
    ProductId, StrategyInstanceId, TargetId,
};
use crate::units::{BaseQuantity, Contracts, Decimal, Money, Price, ReceivedTimeMs, SourceTimeMs};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;
use std::fmt;

/// Current fact envelope schema. Other versions require an explicit migration.
pub const SCHEMA_VERSION: u16 = 1;
/// Current market-event identity rule version. Actual per-channel construction
/// rules belong to the later `market` package.
pub const IDENTITY_VERSION: u16 = 1;

/// Invalid or incompatible fact. Raw external values are not echoed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactError {
    SchemaVersion,
    IdentityVersion,
    InvalidBinding,
    DuplicateInput,
    InvalidTime,
    InvalidAmount,
    InvalidPosition,
    InvalidOrder,
}

impl fmt::Display for FactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid fact: {self:?}")
    }
}

impl std::error::Error for FactError {}

/// Semantic absence or trust state of a read-only fact. `Fresh` may contain
/// an explicit zero; `Missing`, `Stale` and `Unknown` never imply zero.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "snake_case")]
pub enum Availability<T> {
    /// No fact was observed.
    Missing,
    /// Fact is within the caller's configured age limit.
    Fresh(T),
    /// Previously observed fact exceeded the caller's age limit.
    Stale(T),
    /// A request or recovery attempt has an unresolved outcome.
    Unknown,
}

/// Product class admitted by the initial system. Linear swap settlement is
/// still checked against the instrument specification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductKind {
    /// Spot base-currency quantity.
    Spot,
    /// USDT-settled linear perpetual contracts.
    LinearUsdtSwap,
}

/// One already-normalized business event. Raw exchange JSON is intentionally
/// outside this stable fact and must be retained separately by an adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketEvent {
    /// Stable event ID within the declared identity rule version.
    pub event_id: EventId,
    /// Internal product mapping.
    pub product_id: ProductId,
    /// Exchange instrument identity for cross-checking the mapping.
    pub okx_inst_id: OkxInstrumentId,
    /// Product kind checked by the source adapter.
    pub product_kind: ProductKind,
    /// Feed/channel identity.
    pub feed_id: FeedId,
    /// Source connection/session identity for traceability, not deduplication.
    pub source_session: String,
    /// Exchange source timestamp in UTC Unix milliseconds.
    pub source_time_ms: SourceTimeMs,
    /// Local receipt timestamp in UTC Unix milliseconds.
    pub received_time_ms: ReceivedTimeMs,
    /// Optional source sequence; it is not an event ID or global order.
    pub source_sequence: Option<i64>,
}

/// Absolute target state. `Spot(0)` and `SwapLegs(0,0)` are explicit zero
/// targets; `NoSignal` does not extend a previous target's validity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum TargetValue {
    /// No new target this observation.
    NoSignal,
    /// Input or strategy state cannot support a target.
    Unavailable,
    /// Absolute spot base quantity.
    Spot(BaseQuantity),
    /// Net swap target with explicit direction and nonnegative magnitude.
    SwapNet {
        direction: NetDirection,
        contracts: Contracts,
    },
    /// Independent long and short target magnitudes.
    SwapLegs { long: Contracts, short: Contracts },
}

/// Net swap direction; flat requires a zero magnitude.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetDirection {
    /// Positive net contracts.
    Long,
    /// Negative net contracts, expressed as a nonnegative magnitude.
    Short,
    /// Exactly zero contracts.
    Flat,
}

/// Immutable strategy target business fields. Ownership generation and
/// delivery attempts belong to the transport envelope, not the target ID.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyTarget {
    /// Stable business target identity.
    pub target_id: TargetId,
    /// Account whose inventory is targeted.
    pub account_id: AccountId,
    /// Unique strategy instance and inventory owner.
    pub strategy_instance_id: StrategyInstanceId,
    /// Output product.
    pub product_id: ProductId,
    /// Target-role binding.
    pub binding_id: BindingId,
    /// Frozen binding version.
    pub binding_version: String,
    /// Frozen strategy code version.
    pub strategy_version: String,
    /// Frozen business configuration version.
    pub config_version: String,
    /// Frozen policy version.
    pub policy_version: String,
    /// Input event identities in canonical order.
    pub input_event_ids: Vec<EventId>,
    /// Input source watermark, UTC Unix milliseconds.
    pub source_time_ms: SourceTimeMs,
    /// Last UTC Unix millisecond in which this target may be considered.
    pub valid_until_ms: SourceTimeMs,
    /// Target state and its exact unit.
    pub value: TargetValue,
}

impl StrategyTarget {
    /// Checks expiry against a caller-provided UTC Unix millisecond clock.
    /// This never extends validity or approves an order.
    pub fn is_expired(&self, now_ms: SourceTimeMs) -> bool {
        now_ms > self.valid_until_ms
    }
}

/// Actual OKX position mode observed for an account.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionMode {
    /// One net position per swap product.
    NetMode,
    /// Independent long and short positions.
    LongShortMode,
}

/// Position side in an account snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionSide {
    /// Net-mode swap side.
    Net,
    /// Long leg.
    Long,
    /// Short leg.
    Short,
}

/// One exchange-observed position. Spot uses `None` side; swap always has a
/// side matching the account mode.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    /// Internal product identity.
    pub product_id: ProductId,
    /// SPOT or USDT linear SWAP.
    pub product_kind: ProductKind,
    /// Present for a SWAP position only.
    pub side: Option<PositionSide>,
    /// Amount whose variant must match the product kind.
    pub quantity: PositionQuantity,
}

/// Position quantity with no implicit spot/contract conversion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PositionQuantity {
    /// Spot base-currency amount.
    Spot(BaseQuantity),
    /// Swap contract count.
    Swap(Contracts),
}

/// Account snapshot fields needed across package boundaries. It does not
/// claim that a private stream or source is currently trustworthy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountSnapshot {
    /// Local account identity.
    pub account_id: AccountId,
    /// Actual exchange-observed mode.
    pub position_mode: PositionMode,
    /// Account equity with explicit currency.
    pub equity: Money,
    /// Exchange-reported available amount with explicit currency.
    pub available: Money,
    /// Observed positions; duplicate product/side pairs are invalid.
    pub positions: Vec<Position>,
    /// Exchange source UTC Unix milliseconds.
    pub source_time_ms: SourceTimeMs,
    /// Local receipt UTC Unix milliseconds.
    pub received_time_ms: ReceivedTimeMs,
}

/// Exchange order lifecycle report; `Unknown` requires reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    /// Remote outcome cannot currently be proved.
    Unknown,
    /// Accepted and still working.
    Live,
    /// Partially filled and still working.
    PartiallyFilled,
    /// Fully filled terminal state.
    Filled,
    /// Canceled terminal state.
    Canceled,
    /// Rejected terminal state.
    Rejected,
}

/// Exchange-observed order state, not a locally approved order intent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderSnapshot {
    /// Local account identity.
    pub account_id: AccountId,
    /// Internal product identity.
    pub product_id: ProductId,
    /// Stable submitted client ID.
    pub client_order_id: ClientOrderId,
    /// Assigned exchange order ID, absent before remote confirmation.
    pub exchange_order_id: Option<ExchangeOrderId>,
    /// Exchange-observed status.
    pub status: OrderStatus,
    /// Cumulative executed quantity in product units.
    pub filled_quantity: Decimal,
    /// Exchange source UTC Unix milliseconds.
    pub source_time_ms: SourceTimeMs,
}

/// Exchange-observed individual fill; callers deduplicate by account, order
/// and fill identity before local accounting.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FillSnapshot {
    /// Local account identity.
    pub account_id: AccountId,
    /// Internal product identity.
    pub product_id: ProductId,
    /// Parent client order identity.
    pub client_order_id: ClientOrderId,
    /// Exchange fill identity.
    pub fill_id: FillId,
    /// Positive executed quantity in product units.
    pub quantity: Decimal,
    /// Execution price with explicit base/quote denomination.
    pub price: Price,
    /// Actual fee; sign and currency are retained.
    pub fee: Money,
    /// Exchange source UTC Unix milliseconds.
    pub source_time_ms: SourceTimeMs,
}

/// Version-one payload variants. Unknown variants are rejected by Serde.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Fact {
    /// Normalized market observation.
    MarketEvent(MarketEvent),
    /// Strategy output.
    StrategyTarget(StrategyTarget),
    /// Exchange account observation.
    AccountSnapshot(AccountSnapshot),
    /// Exchange order observation.
    OrderSnapshot(OrderSnapshot),
    /// Exchange fill observation.
    FillSnapshot(FillSnapshot),
}

/// Versioned fact envelope. Deserialization validates versions and cross-field
/// invariants; old or unknown versions require a separately reviewed migration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FactEnvelope {
    /// Fact schema version, currently one.
    schema_version: u16,
    /// Market-event identity rule version, currently one.
    identity_version: u16,
    /// Typed fact body.
    fact: Fact,
}

impl FactEnvelope {
    /// Builds a validated current-version fact without I/O or side effects.
    pub fn new(fact: Fact) -> Result<Self, FactError> {
        validate(&fact)?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            identity_version: IDENTITY_VERSION,
            fact,
        })
    }

    /// Returns the current version-one typed fact.
    pub fn fact(&self) -> &Fact {
        &self.fact
    }
}

impl<'de> Deserialize<'de> for FactEnvelope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            schema_version: u16,
            identity_version: u16,
            fact: Fact,
        }
        let wire = Wire::deserialize(deserializer)?;
        if wire.schema_version != SCHEMA_VERSION {
            return Err(serde::de::Error::custom(FactError::SchemaVersion));
        }
        if wire.identity_version != IDENTITY_VERSION {
            return Err(serde::de::Error::custom(FactError::IdentityVersion));
        }
        Self::new(wire.fact).map_err(serde::de::Error::custom)
    }
}

fn validate(fact: &Fact) -> Result<(), FactError> {
    match fact {
        Fact::MarketEvent(event) => {
            if event.source_sequence.is_some_and(|sequence| sequence < 0)
                || event.source_session.is_empty()
                || event.source_session.len() > 128
            {
                return Err(FactError::InvalidTime);
            }
        }
        Fact::StrategyTarget(target) => {
            if target.source_time_ms > target.valid_until_ms {
                return Err(FactError::InvalidTime);
            }
            if target.strategy_version.is_empty()
                || target.config_version.is_empty()
                || target.binding_version.is_empty()
                || target.policy_version.is_empty()
            {
                return Err(FactError::InvalidBinding);
            }
            let mut ids = HashSet::new();
            if target.input_event_ids.iter().any(|id| !ids.insert(id)) {
                return Err(FactError::DuplicateInput);
            }
            match &target.value {
                TargetValue::Spot(q) if q.product_id() != &target.product_id => {
                    return Err(FactError::InvalidBinding);
                }
                TargetValue::SwapNet {
                    direction,
                    contracts,
                } => {
                    if contracts.product_id() != &target.product_id
                        || (*direction == NetDirection::Flat)
                            != (contracts.amount().to_numeric_text() == "0")
                    {
                        return Err(FactError::InvalidBinding);
                    }
                }
                TargetValue::SwapLegs { long, short }
                    if long.product_id() != &target.product_id
                        || short.product_id() != &target.product_id =>
                {
                    return Err(FactError::InvalidBinding);
                }
                _ => {}
            }
        }
        Fact::AccountSnapshot(account) => {
            if account.equity.currency != account.available.currency {
                return Err(FactError::InvalidAmount);
            }
            let mut keys = HashSet::new();
            for position in &account.positions {
                if !keys.insert((&position.product_id, position.side)) {
                    return Err(FactError::InvalidPosition);
                }
                let valid = match (
                    &position.product_kind,
                    &position.side,
                    &position.quantity,
                    account.position_mode,
                ) {
                    (ProductKind::Spot, None, PositionQuantity::Spot(q), _) => {
                        q.product_id() == &position.product_id
                    }
                    (
                        ProductKind::LinearUsdtSwap,
                        Some(PositionSide::Net),
                        PositionQuantity::Swap(q),
                        PositionMode::NetMode,
                    ) => q.product_id() == &position.product_id,
                    (
                        ProductKind::LinearUsdtSwap,
                        Some(PositionSide::Long | PositionSide::Short),
                        PositionQuantity::Swap(q),
                        PositionMode::LongShortMode,
                    ) => q.product_id() == &position.product_id,
                    _ => false,
                };
                if !valid {
                    return Err(FactError::InvalidPosition);
                }
            }
        }
        Fact::OrderSnapshot(order) => {
            if order.filled_quantity.is_negative()
                || (order.status == OrderStatus::Filled
                    && order.filled_quantity.to_numeric_text() == "0")
            {
                return Err(FactError::InvalidOrder);
            }
        }
        Fact::FillSnapshot(fill) => {
            if fill.quantity.is_negative() || fill.quantity.to_numeric_text() == "0" {
                return Err(FactError::InvalidAmount);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Currency;

    fn id<T: std::str::FromStr>(value: &str) -> T
    where
        T::Err: fmt::Debug,
    {
        value.parse().expect("valid test ID")
    }

    fn target(value: TargetValue) -> StrategyTarget {
        StrategyTarget {
            target_id: id("target-1"),
            account_id: id("account-1"),
            strategy_instance_id: id("instance-1"),
            product_id: id("spot-a"),
            binding_id: id("binding-1"),
            binding_version: "v1".into(),
            strategy_version: "v1".into(),
            config_version: "v1".into(),
            policy_version: "v1".into(),
            input_event_ids: vec![id("event-1")],
            source_time_ms: SourceTimeMs::new(100).expect("time"),
            valid_until_ms: SourceTimeMs::new(200).expect("time"),
            value,
        }
    }

    #[test]
    fn zero_missing_stale_and_unknown_remain_distinct() {
        let zero =
            BaseQuantity::new(Decimal::parse("0").expect("zero"), id("spot-a")).expect("quantity");
        let fact =
            FactEnvelope::new(Fact::StrategyTarget(target(TargetValue::Spot(zero)))).expect("fact");
        let wire = serde_json::to_string(&fact).expect("serialize");
        assert_eq!(
            serde_json::from_str::<FactEnvelope>(&wire).expect("decode"),
            fact
        );
        if let Fact::StrategyTarget(target) = &fact.fact {
            assert!(!target.is_expired(SourceTimeMs::new(200).expect("time")));
            assert!(target.is_expired(SourceTimeMs::new(201).expect("time")));
        }
        let missing: Availability<Decimal> = Availability::Missing;
        let stale: Availability<Decimal> = Availability::Stale(Decimal::parse("0").expect("zero"));
        let unknown: Availability<Decimal> = Availability::Unknown;
        assert_ne!(missing, stale);
        assert_ne!(stale, unknown);
        assert_ne!(
            target(TargetValue::NoSignal).value,
            target(TargetValue::Unavailable).value
        );
    }

    #[test]
    fn versions_and_unknown_fields_are_rejected() {
        let fact =
            FactEnvelope::new(Fact::StrategyTarget(target(TargetValue::NoSignal))).expect("fact");
        let mut wire = serde_json::to_value(fact).expect("serialize");
        for version in [0, 2] {
            wire["schema_version"] = version.into();
            assert!(serde_json::from_value::<FactEnvelope>(wire.clone()).is_err());
        }
        wire["schema_version"] = 1.into();
        wire["identity_version"] = 2.into();
        assert!(serde_json::from_value::<FactEnvelope>(wire.clone()).is_err());
        wire["identity_version"] = 1.into();
        wire["unexpected"] = true.into();
        assert!(serde_json::from_value::<FactEnvelope>(wire).is_err());
    }

    #[test]
    fn duplicate_events_and_cross_product_quantities_are_rejected() {
        let mut duplicate = target(TargetValue::NoSignal);
        duplicate.input_event_ids.push(id("event-1"));
        assert_eq!(
            FactEnvelope::new(Fact::StrategyTarget(duplicate)),
            Err(FactError::DuplicateInput)
        );
        let cross_product =
            BaseQuantity::new(Decimal::parse("1").expect("one"), id("spot-b")).expect("quantity");
        assert_eq!(
            FactEnvelope::new(Fact::StrategyTarget(target(TargetValue::Spot(
                cross_product
            )))),
            Err(FactError::InvalidBinding)
        );
    }

    #[test]
    fn account_side_and_currency_are_checked() {
        let money = |currency| Money {
            amount: Decimal::parse("0").expect("zero"),
            currency: Currency::new(currency).expect("currency"),
        };
        let mut account = AccountSnapshot {
            account_id: id("account-1"),
            position_mode: PositionMode::NetMode,
            equity: money("USDT"),
            available: money("BTC"),
            positions: vec![],
            source_time_ms: SourceTimeMs::new(1).expect("time"),
            received_time_ms: ReceivedTimeMs::new(1).expect("time"),
        };
        assert_eq!(
            FactEnvelope::new(Fact::AccountSnapshot(account.clone())),
            Err(FactError::InvalidAmount)
        );
        account.available = money("USDT");
        let quantity =
            Contracts::new(Decimal::parse("1").expect("one"), id("swap-a")).expect("contracts");
        account.positions.push(Position {
            product_id: id("swap-a"),
            product_kind: ProductKind::LinearUsdtSwap,
            side: Some(PositionSide::Long),
            quantity: PositionQuantity::Swap(quantity),
        });
        assert_eq!(
            FactEnvelope::new(Fact::AccountSnapshot(account)),
            Err(FactError::InvalidPosition)
        );
    }
}
