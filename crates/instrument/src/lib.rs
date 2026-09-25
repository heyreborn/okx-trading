//! Pure validation of read-only OKX demo product and account directory facts.
//! Mapping, eligibility, specification and fee freshness must all agree before
//! a product is considered usable for later planning. See the [README](../README.md).

use model::facts::{PositionMode, ProductKind};
use model::identity::{AccountId, OkxInstrumentId, ProductId};
use model::units::{Currency, Decimal, ReceivedTimeMs, SourceTimeMs};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Directory validation failure. Supplied exchange values are never printed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    DuplicateMapping,
    DuplicateRemote,
    MissingSpecification,
    MappingMismatch,
    UnsupportedKind,
    InvalidSpecification,
    InvalidFee,
    Inactive,
    StaleSpecification,
    StaleEligibility,
    StaleFee,
    AccountMismatch,
    ModeNotAllowed,
    ProductNotAllowed,
    RemoteNotEligible,
    TimeInFuture,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unusable instrument directory fact: {self:?}")
    }
}

impl std::error::Error for Error {}

/// Immutable local mapping to one demo OKX instrument. A different `instId`
/// or product kind requires a new internal product ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductMapping {
    /// Stable internal ID.
    pub product_id: ProductId,
    /// OKX demo `instId`.
    pub okx_inst_id: OkxInstrumentId,
    /// Supported kind.
    pub kind: ProductKind,
    /// Base currency.
    pub base: Currency,
    /// Quote currency.
    pub quote: Currency,
    /// SWAP settlement currency, absent for SPOT.
    pub settlement: Option<Currency>,
}

/// Exchange trading state. `PostOnly` cannot be used for IOC or ordinary
/// limit orders and is therefore rejected by the initial read-only gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TradingState {
    /// Normal active state.
    Live,
    /// Only post-only orders accepted.
    PostOnly,
    /// Suspended or rebasing state.
    Inactive,
}

/// Public exchange specification parsed by the OKX adapter. `lot_size` and
/// `min_size` are base currency units for SPOT and contracts for SWAP.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstrumentSpec {
    /// Exchange instrument ID.
    pub okx_inst_id: OkxInstrumentId,
    /// Exchange product kind; SWAP must be USDT linear.
    pub kind: ProductKind,
    /// Base currency from the exchange.
    pub base: Currency,
    /// Quote currency from the exchange.
    pub quote: Currency,
    /// Contract settlement currency, absent for SPOT.
    pub settlement: Option<Currency>,
    /// Version derived from the complete normalized specification.
    pub version: String,
    /// Exchange state.
    pub state: TradingState,
    /// Price tick in quote currency per base unit.
    pub tick_size: Decimal,
    /// Quantity lot in base units or contracts.
    pub lot_size: Decimal,
    /// Minimum order quantity in the same unit as `lot_size`.
    pub min_size: Decimal,
    /// Base units per contract for SWAP; absent for SPOT.
    pub contract_value: Option<Decimal>,
    /// Currency of `contract_value`, absent for SPOT.
    pub contract_value_currency: Option<Currency>,
    /// Exchange fee group ID, required to bind current rates.
    pub fee_group_id: Option<String>,
    /// Exchange source UTC Unix milliseconds.
    pub source_time_ms: SourceTimeMs,
    /// Local receipt UTC Unix milliseconds.
    pub received_time_ms: ReceivedTimeMs,
}

/// Local account permission from published configuration. It never substitutes
/// for actual exchange account mode or product eligibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountPermit {
    /// Local account ID.
    pub account_id: AccountId,
    /// Locally permitted actual modes.
    pub allowed_modes: Vec<PositionMode>,
    /// Locally permitted products.
    pub product_ids: Vec<ProductId>,
}

/// Actual read-only account directory observation from OKX.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountEligibility {
    /// Local account identity that owns the credentials.
    pub account_id: AccountId,
    /// Observed actual account mode.
    pub position_mode: PositionMode,
    /// Exchange instrument IDs available to this account.
    pub instrument_ids: Vec<OkxInstrumentId>,
    /// Exchange source UTC Unix milliseconds.
    pub source_time_ms: SourceTimeMs,
    /// Local receipt UTC Unix milliseconds.
    pub received_time_ms: ReceivedTimeMs,
}

/// Actual per-product fee fact. Rates retain OKX's sign convention: positive
/// means rebate and negative means commission. Planners must select the
/// applicable side/group rather than assume one fixed rate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeSchedule {
    /// Exchange instrument ID this fee applies to.
    pub okx_inst_id: OkxInstrumentId,
    /// Fee group selected from the matching specification.
    pub group_id: String,
    /// Version of the normalized fee fact.
    pub version: String,
    /// Signed maker fee fraction.
    pub maker: Decimal,
    /// Signed taker fee fraction.
    pub taker: Decimal,
    /// Exchange source UTC Unix milliseconds.
    pub source_time_ms: SourceTimeMs,
    /// Local receipt UTC Unix milliseconds.
    pub received_time_ms: ReceivedTimeMs,
}

/// Pure directory assembled from one local mapping set and one fresh public
/// specification set. It cannot place orders or grant account permissions.
pub struct Directory {
    mappings: HashMap<ProductId, ProductMapping>,
    specs: HashMap<OkxInstrumentId, InstrumentSpec>,
}

impl Directory {
    /// Validates uniqueness, mapping consistency, product units and required
    /// specifications. No network calls or persistent writes occur.
    pub fn new(mappings: Vec<ProductMapping>, specs: Vec<InstrumentSpec>) -> Result<Self, Error> {
        let mut by_product = HashMap::new();
        let mut remote_ids = HashSet::new();
        for mapping in mappings {
            if !remote_ids.insert(mapping.okx_inst_id.clone()) {
                return Err(Error::DuplicateMapping);
            }
            if by_product
                .insert(mapping.product_id.clone(), mapping)
                .is_some()
            {
                return Err(Error::DuplicateMapping);
            }
        }
        let mut by_remote = HashMap::new();
        for spec in specs {
            if by_remote.insert(spec.okx_inst_id.clone(), spec).is_some() {
                return Err(Error::DuplicateRemote);
            }
        }
        for mapping in by_product.values() {
            let spec = by_remote
                .get(&mapping.okx_inst_id)
                .ok_or(Error::MissingSpecification)?;
            validate_spec(mapping, spec)?;
        }
        Ok(Self {
            mappings: by_product,
            specs: by_remote,
        })
    }

    /// Checks local and remote account eligibility, state, fee match and fact
    /// age at a caller-supplied UTC time. Success is only a read-only directory
    /// result; it does not authorize a strategy target or an order.
    pub fn assess(
        &self,
        product_id: &ProductId,
        permit: &AccountPermit,
        eligibility: &AccountEligibility,
        fee: &FeeSchedule,
        now_ms: ReceivedTimeMs,
        max_age_ms: i64,
    ) -> Result<&InstrumentSpec, Error> {
        let mapping = self
            .mappings
            .get(product_id)
            .ok_or(Error::MissingSpecification)?;
        let spec = self
            .specs
            .get(&mapping.okx_inst_id)
            .ok_or(Error::MissingSpecification)?;
        if permit.account_id != eligibility.account_id {
            return Err(Error::AccountMismatch);
        }
        if !permit.allowed_modes.contains(&eligibility.position_mode) {
            return Err(Error::ModeNotAllowed);
        }
        if !permit.product_ids.contains(product_id) {
            return Err(Error::ProductNotAllowed);
        }
        if !eligibility.instrument_ids.contains(&mapping.okx_inst_id) {
            return Err(Error::RemoteNotEligible);
        }
        if spec.state != TradingState::Live {
            return Err(Error::Inactive);
        }
        fresh(
            now_ms,
            spec.source_time_ms,
            spec.received_time_ms,
            max_age_ms,
        )
        .map_err(|e| {
            if e == Error::TimeInFuture {
                e
            } else {
                Error::StaleSpecification
            }
        })?;
        fresh(
            now_ms,
            eligibility.source_time_ms,
            eligibility.received_time_ms,
            max_age_ms,
        )
        .map_err(|e| {
            if e == Error::TimeInFuture {
                e
            } else {
                Error::StaleEligibility
            }
        })?;
        if fee.okx_inst_id != mapping.okx_inst_id
            || fee.version.is_empty()
            || spec.fee_group_id.as_deref() != Some(fee.group_id.as_str())
        {
            return Err(Error::InvalidFee);
        }
        fresh(now_ms, fee.source_time_ms, fee.received_time_ms, max_age_ms).map_err(|e| {
            if e == Error::TimeInFuture {
                e
            } else {
                Error::StaleFee
            }
        })?;
        Ok(spec)
    }
}

fn validate_spec(mapping: &ProductMapping, spec: &InstrumentSpec) -> Result<(), Error> {
    if mapping.kind != spec.kind
        || mapping.base != spec.base
        || mapping.quote != spec.quote
        || mapping.settlement != spec.settlement
    {
        return Err(Error::MappingMismatch);
    }
    if spec.version.is_empty()
        || spec.version.len() > 128
        || spec.tick_size.is_negative()
        || spec.tick_size.to_numeric_text() == "0"
        || spec.lot_size.is_negative()
        || spec.lot_size.to_numeric_text() == "0"
        || spec.min_size.is_negative()
        || spec.min_size.to_numeric_text() == "0"
        || spec.source_time_ms.get() > spec.received_time_ms.get()
    {
        return Err(Error::InvalidSpecification);
    }
    match spec.kind {
        ProductKind::Spot
            if spec.settlement.is_none()
                && spec.contract_value.is_none()
                && spec.contract_value_currency.is_none() =>
        {
            Ok(())
        }
        ProductKind::LinearUsdtSwap
            if spec
                .settlement
                .as_ref()
                .is_some_and(|c| c.as_str() == "USDT")
                && spec
                    .contract_value
                    .as_ref()
                    .is_some_and(|v| !v.is_negative() && v.to_numeric_text() != "0")
                && spec.contract_value_currency.as_ref() == Some(&spec.base) =>
        {
            Ok(())
        }
        _ => Err(Error::UnsupportedKind),
    }
}

fn fresh(
    now: ReceivedTimeMs,
    source: SourceTimeMs,
    received: ReceivedTimeMs,
    max_age_ms: i64,
) -> Result<(), Error> {
    let source_age = now
        .get()
        .checked_sub(source.get())
        .ok_or(Error::TimeInFuture)?;
    let receipt_age = now
        .get()
        .checked_sub(received.get())
        .ok_or(Error::TimeInFuture)?;
    if source_age < 0 || receipt_age < 0 || source.get() > received.get() {
        return Err(Error::TimeInFuture);
    }
    if max_age_ms < 0 || source_age > max_age_ms || receipt_age > max_age_ms {
        return Err(Error::StaleSpecification);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn id<T: FromStr>(value: &str) -> T
    where
        T::Err: fmt::Debug,
    {
        value.parse().expect("valid ID")
    }
    fn ccy(value: &str) -> Currency {
        Currency::new(value).expect("currency")
    }
    fn dec(value: &str) -> Decimal {
        Decimal::parse(value).expect("decimal")
    }
    fn at(value: i64) -> (SourceTimeMs, ReceivedTimeMs) {
        (
            SourceTimeMs::new(value).expect("source time"),
            ReceivedTimeMs::new(value).expect("receipt time"),
        )
    }
    fn spot() -> (ProductMapping, InstrumentSpec) {
        let (source_time_ms, received_time_ms) = at(100);
        let mapping = ProductMapping {
            product_id: id("spot-a"),
            okx_inst_id: id("BTC-USDT"),
            kind: ProductKind::Spot,
            base: ccy("BTC"),
            quote: ccy("USDT"),
            settlement: None,
        };
        let spec = InstrumentSpec {
            okx_inst_id: mapping.okx_inst_id.clone(),
            kind: ProductKind::Spot,
            base: ccy("BTC"),
            quote: ccy("USDT"),
            settlement: None,
            version: "v1".into(),
            state: TradingState::Live,
            tick_size: dec("0.1"),
            lot_size: dec("0.0001"),
            min_size: dec("0.001"),
            contract_value: None,
            contract_value_currency: None,
            fee_group_id: Some("1".into()),
            source_time_ms,
            received_time_ms,
        };
        (mapping, spec)
    }

    #[test]
    fn mapping_and_specs_fail_closed() {
        let (mapping, spec) = spot();
        assert!(matches!(
            Directory::new(vec![mapping.clone(), mapping.clone()], vec![spec.clone()]),
            Err(Error::DuplicateMapping)
        ));
        assert!(matches!(
            Directory::new(vec![mapping.clone()], vec![spec.clone(), spec.clone()]),
            Err(Error::DuplicateRemote)
        ));
        let mut bad = spec.clone();
        bad.kind = ProductKind::LinearUsdtSwap;
        assert!(matches!(
            Directory::new(vec![mapping.clone()], vec![bad]),
            Err(Error::MappingMismatch)
        ));
        let mut bad = spec;
        bad.tick_size = dec("0");
        assert!(matches!(
            Directory::new(vec![mapping], vec![bad]),
            Err(Error::InvalidSpecification)
        ));
    }

    #[test]
    fn stale_state_mode_and_permission_are_distinct() {
        let (mapping, spec) = spot();
        let directory = Directory::new(vec![mapping.clone()], vec![spec]).expect("directory");
        let permit = AccountPermit {
            account_id: id("account-a"),
            allowed_modes: vec![PositionMode::NetMode],
            product_ids: vec![mapping.product_id.clone()],
        };
        let (source_time_ms, received_time_ms) = at(100);
        let mut eligibility = AccountEligibility {
            account_id: permit.account_id.clone(),
            position_mode: PositionMode::NetMode,
            instrument_ids: vec![mapping.okx_inst_id.clone()],
            source_time_ms,
            received_time_ms,
        };
        let mut fee = FeeSchedule {
            okx_inst_id: mapping.okx_inst_id,
            group_id: "1".into(),
            version: "v1".into(),
            maker: dec("-0.0001"),
            taker: dec("0.001"),
            source_time_ms,
            received_time_ms,
        };
        let now = ReceivedTimeMs::new(110).expect("time");
        assert!(
            directory
                .assess(&mapping.product_id, &permit, &eligibility, &fee, now, 10)
                .is_ok()
        );
        fee.group_id = "other".into();
        assert!(matches!(
            directory.assess(&mapping.product_id, &permit, &eligibility, &fee, now, 10),
            Err(Error::InvalidFee)
        ));
        fee.group_id = "1".into();
        assert!(matches!(
            directory.assess(&mapping.product_id, &permit, &eligibility, &fee, now, 9),
            Err(Error::StaleSpecification)
        ));
        eligibility.position_mode = PositionMode::LongShortMode;
        assert!(matches!(
            directory.assess(&mapping.product_id, &permit, &eligibility, &fee, now, 10),
            Err(Error::ModeNotAllowed)
        ));
        eligibility.position_mode = PositionMode::NetMode;
        eligibility.instrument_ids.clear();
        assert!(matches!(
            directory.assess(&mapping.product_id, &permit, &eligibility, &fee, now, 10),
            Err(Error::RemoteNotEligible)
        ));
        eligibility.instrument_ids.push(fee.okx_inst_id.clone());
        eligibility.source_time_ms = SourceTimeMs::new(99).expect("older source");
        assert!(matches!(
            directory.assess(&mapping.product_id, &permit, &eligibility, &fee, now, 10),
            Err(Error::StaleEligibility)
        ));
        eligibility.source_time_ms = source_time_ms;
        fee.source_time_ms = SourceTimeMs::new(99).expect("older fee");
        assert!(matches!(
            directory.assess(&mapping.product_id, &permit, &eligibility, &fee, now, 10),
            Err(Error::StaleFee)
        ));
    }
}
