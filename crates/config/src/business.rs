//! Strict, versioned demo business configuration. Parsing is pure and
//! deterministic; remote account and instrument facts need separate checks.

use model::facts::{PositionMode, ProductKind};
use model::identity::{AccountId, BindingId, OkxInstrumentId, ProductId, StrategyInstanceId};
use model::units::{Currency, Ratio};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Strict configuration error. Raw TOML and secret values are never printed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Syntax,
    SchemaVersion,
    Environment,
    TradingEnabled,
    EmptyCollection,
    DuplicateId,
    ConflictingInstrument,
    UnknownProduct,
    UnknownAccount,
    UnlicensedProduct,
    InvalidBinding,
    UnsupportedCapability,
    InvalidPolicy,
    Digest,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid business configuration: {self:?}")
    }
}

impl std::error::Error for Error {}

/// Instrument whitelist entry. Mapping must remain immutable once published.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Product {
    /// Stable internal product ID.
    pub product_id: ProductId,
    /// OKX `instId` in the demo namespace.
    pub okx_inst_id: OkxInstrumentId,
    /// Supported product kind.
    pub kind: ProductKind,
    /// Base currency.
    pub base: Currency,
    /// Quote currency.
    pub quote: Currency,
    /// Settlement currency for SWAP only.
    pub settlement: Option<Currency>,
}

/// Local account allowlist. Actual OKX mode and eligibility must be checked
/// again from a fresh read-only account response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    /// Local account ID.
    pub account_id: AccountId,
    /// Permitted actual position modes; does not set the remote mode.
    pub allowed_position_modes: Vec<PositionMode>,
    /// Products this account may use after remote eligibility checks.
    pub product_ids: Vec<ProductId>,
}

/// Static first-round strategy implementations; runtime registration in
/// `strategy` must match before any strategy can run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyKind {
    /// Consumes a closed one-minute candle.
    CandleMomentum,
    /// Consumes the unaggregated trades feed.
    TradeFlow,
}

/// Feed types supported by the initial static capability declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedKind {
    /// Closed one-minute candles; live partial updates are a later concern.
    Candle1m,
    /// Unaggregated trades.
    TradesAll,
}

/// Target mode supported by the declared product/account relationship.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetMode {
    /// Absolute spot base quantity.
    Spot,
    /// One signed net swap target.
    SwapNet,
    /// Separate long and short swap targets.
    SwapLegs,
}

/// One strategy input binding, distinct from a target permission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputBinding {
    /// Stable binding identity.
    pub binding_id: BindingId,
    /// Source product.
    pub product_id: ProductId,
    /// Required feed.
    pub feed: FeedKind,
}

/// One strategy output binding to a permitted product.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetBinding {
    /// Stable binding identity.
    pub binding_id: BindingId,
    /// Target product.
    pub product_id: ProductId,
    /// Absolute target unit and mode.
    pub mode: TargetMode,
}

/// One independently owned strategy instance. Multiple instances may use the
/// same implementation and product without sharing state or inventory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyInstance {
    /// Unique instance ID.
    pub strategy_instance_id: StrategyInstanceId,
    /// Static implementation capability.
    pub kind: StrategyKind,
    /// Owning local account.
    pub account_id: AccountId,
    /// Version of this instance's parameter schema and values.
    pub parameter_version: String,
    /// Version of input and output bindings.
    pub binding_version: String,
    /// Maximum fraction of account equity assigned to this instance.
    pub budget_fraction: Ratio,
    /// One or more input feeds; cross-product inputs are allowed.
    pub inputs: Vec<InputBinding>,
    /// One or more target products.
    pub targets: Vec<TargetBinding>,
}

/// Versioned account-level risk limits. These are local policy, not OKX facts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskPolicy {
    /// Stable policy version.
    pub version: String,
    /// Upper bound on one order's fraction of equity.
    pub max_order_fraction: Ratio,
    /// Upper bound on total account gross exposure fraction.
    pub max_account_gross_fraction: Ratio,
    /// Upper bound on daily loss fraction.
    pub max_daily_loss_fraction: Ratio,
}

/// Initial conflict rule for multiple strategy instances.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombinationPolicy {
    /// Reject simultaneous opposing desired adjustments.
    RejectConflicts,
}

/// Local trading gate. Stage one only accepts disabled execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TradingGate {
    /// Must be false until later explicit acceptance stages.
    #[serde(default)]
    pub enabled: bool,
}

/// Strict schema-one TOML representation before validation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessFile {
    /// Must equal one.
    pub schema_version: u16,
    /// Immutable publication version.
    pub version: String,
    /// Must be `demo`.
    pub environment: String,
    /// Disabled trading gate.
    #[serde(default)]
    pub trading: TradingGate,
    /// Product whitelist.
    pub products: Vec<Product>,
    /// Local accounts and product permissions.
    pub accounts: Vec<Account>,
    /// Strategy instances and explicit bindings.
    pub strategies: Vec<StrategyInstance>,
    /// Local risk limits.
    pub risk: RiskPolicy,
    /// Multi-instance combination policy.
    pub combination: CombinationPolicy,
}

/// Validated immutable business config and SHA-256 digest of its canonical
/// JSON structure. Array order is semantically significant in this version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusinessConfig {
    /// Validated schema-one values.
    file: BusinessFile,
    /// Lowercase SHA-256 digest of the parsed, serialized structure.
    digest: String,
}

impl BusinessConfig {
    /// Returns the validated immutable values.
    pub fn file(&self) -> &BusinessFile {
        &self.file
    }

    /// Returns the canonical content digest for multi-node comparison.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Checks a deployment-supplied expected digest without exposing content.
    pub fn verify_digest(&self, expected: &str) -> Result<(), Error> {
        (self.digest == expected).then_some(()).ok_or(Error::Digest)
    }
}

/// Parses and validates TOML without file, environment or network I/O.
/// Errors reveal categories only; supplied values are not echoed.
pub fn parse(source: &str) -> Result<BusinessConfig, Error> {
    let file: BusinessFile = toml::from_str(source).map_err(|_| Error::Syntax)?;
    validate(&file)?;
    let canonical = serde_json::to_vec(&file).map_err(|_| Error::Digest)?;
    let digest = format!("{:x}", Sha256::digest(canonical));
    Ok(BusinessConfig { file, digest })
}

fn validate(file: &BusinessFile) -> Result<(), Error> {
    if file.schema_version != 1 {
        return Err(Error::SchemaVersion);
    }
    if file.environment != "demo" {
        return Err(Error::Environment);
    }
    if file.trading.enabled {
        return Err(Error::TradingEnabled);
    }
    if file.version.is_empty() || file.version.len() > 128 || file.risk.version.is_empty() {
        return Err(Error::InvalidPolicy);
    }
    if file.products.is_empty() || file.accounts.is_empty() || file.strategies.is_empty() {
        return Err(Error::EmptyCollection);
    }
    if file.risk.max_order_fraction.amount() > file.risk.max_account_gross_fraction.amount() {
        return Err(Error::InvalidPolicy);
    }

    let mut products = HashMap::new();
    let mut remote = HashSet::new();
    for product in &file.products {
        if products.insert(&product.product_id, product).is_some() {
            return Err(Error::DuplicateId);
        }
        if !remote.insert(&product.okx_inst_id) {
            return Err(Error::ConflictingInstrument);
        }
        if product.base == product.quote
            || match product.kind {
                ProductKind::Spot => product.settlement.is_some(),
                ProductKind::LinearUsdtSwap => product
                    .settlement
                    .as_ref()
                    .is_none_or(|ccy| ccy.as_str() != "USDT"),
            }
        {
            return Err(Error::InvalidBinding);
        }
    }
    let mut accounts = HashMap::new();
    for account in &file.accounts {
        if accounts.insert(&account.account_id, account).is_some() {
            return Err(Error::DuplicateId);
        }
        if account.product_ids.is_empty() || account.allowed_position_modes.is_empty() {
            return Err(Error::EmptyCollection);
        }
        let mut allowed = HashSet::new();
        for product_id in &account.product_ids {
            if !products.contains_key(product_id) {
                return Err(Error::UnknownProduct);
            }
            if !allowed.insert(product_id) {
                return Err(Error::DuplicateId);
            }
        }
        let modes: HashSet<_> = account.allowed_position_modes.iter().copied().collect();
        if modes.len() != account.allowed_position_modes.len() {
            return Err(Error::DuplicateId);
        }
    }
    let mut instances = HashSet::new();
    let mut bindings = HashSet::new();
    for strategy in &file.strategies {
        if !instances.insert(&strategy.strategy_instance_id) {
            return Err(Error::DuplicateId);
        }
        if strategy.parameter_version.is_empty() || strategy.binding_version.is_empty() {
            return Err(Error::InvalidBinding);
        }
        let account = accounts
            .get(&strategy.account_id)
            .ok_or(Error::UnknownAccount)?;
        if strategy.inputs.is_empty() || strategy.targets.is_empty() {
            return Err(Error::EmptyCollection);
        }
        let mut feed_keys = HashSet::new();
        for input in &strategy.inputs {
            if !bindings.insert(&input.binding_id)
                || !feed_keys.insert((&input.product_id, input.feed))
            {
                return Err(Error::DuplicateId);
            }
            if !products.contains_key(&input.product_id) {
                return Err(Error::UnknownProduct);
            }
            let required = match strategy.kind {
                StrategyKind::CandleMomentum => FeedKind::Candle1m,
                StrategyKind::TradeFlow => FeedKind::TradesAll,
            };
            if input.feed != required {
                return Err(Error::UnsupportedCapability);
            }
        }
        let mut target_keys = HashSet::new();
        for target in &strategy.targets {
            if !bindings.insert(&target.binding_id) || !target_keys.insert(&target.product_id) {
                return Err(Error::DuplicateId);
            }
            let product = products
                .get(&target.product_id)
                .ok_or(Error::UnknownProduct)?;
            if !account.product_ids.contains(&target.product_id) {
                return Err(Error::UnlicensedProduct);
            }
            let compatible = match (product.kind, target.mode) {
                (ProductKind::Spot, TargetMode::Spot)
                | (ProductKind::LinearUsdtSwap, TargetMode::SwapNet) => true,
                (ProductKind::LinearUsdtSwap, TargetMode::SwapLegs) => account
                    .allowed_position_modes
                    .contains(&PositionMode::LongShortMode),
                _ => false,
            };
            if !compatible {
                return Err(Error::UnsupportedCapability);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../config/example.toml");

    #[test]
    fn example_and_formatting_have_stable_digest() {
        let first = parse(SAMPLE).expect("valid example");
        let second = parse(&SAMPLE.replace("schema_version = 1", "schema_version=1"))
            .expect("equivalent TOML");
        assert_eq!(first.digest(), second.digest());
        assert_eq!(first.file().strategies.len(), 3);
        assert!(!first.file().trading.enabled);
        assert!(first.verify_digest(second.digest()).is_ok());
        assert_eq!(first.verify_digest("wrong"), Err(Error::Digest));
    }

    #[test]
    fn duplicate_hanging_and_incompatible_bindings_fail() {
        let duplicate = SAMPLE.replace(
            "strategy_instance_id = \"spot-flow\"",
            "strategy_instance_id = \"spot-candle\"",
        );
        assert_eq!(
            parse(&duplicate).expect_err("duplicate ID"),
            Error::DuplicateId
        );
        let missing = SAMPLE.replace(
            "product_id = \"spot-btc\"\nfeed = \"trades_all\"",
            "product_id = \"missing\"\nfeed = \"trades_all\"",
        );
        assert_eq!(
            parse(&missing).expect_err("unknown product"),
            Error::UnknownProduct
        );
        let wrong = SAMPLE.replace("mode = \"spot\"", "mode = \"swap_legs\"");
        assert_eq!(
            parse(&wrong).expect_err("bad mode"),
            Error::UnsupportedCapability
        );
        let modes = SAMPLE.replace(
            "[\"net_mode\", \"long_short_mode\"]",
            "[\"net_mode\", \"net_mode\"]",
        );
        assert_eq!(
            parse(&modes).expect_err("duplicate mode"),
            Error::DuplicateId
        );
    }

    #[test]
    fn rejects_unknown_fields_live_mode_and_gate() {
        assert!(matches!(
            parse(&SAMPLE.replace("environment = \"demo\"", "environment = \"live\"")),
            Err(Error::Environment)
        ));
        assert!(matches!(
            parse(&SAMPLE.replace("enabled = false", "enabled = true")),
            Err(Error::TradingEnabled)
        ));
        assert!(matches!(
            parse(&format!("{SAMPLE}\nunknown = 1\n")),
            Err(Error::Syntax)
        ));
        assert!(matches!(
            parse(&SAMPLE.replace("schema_version = 1", "schema_version = 2")),
            Err(Error::SchemaVersion)
        ));
    }
}
