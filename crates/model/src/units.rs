//! Exact decimal and timestamp values used across contract boundaries.
//! Decimal wire values are strings in the exact `NUMERIC(38,18)` domain.
//! Rounding is explicit, and a caller must recheck limits after quantization.

use crate::identity::ProductId;
use bigdecimal::{BigDecimal, RoundingMode};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// Rejection at a decimal, unit, or time boundary; input values are not echoed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitError {
    InvalidDecimal,
    OutOfRange,
    Negative,
    ZeroStep,
    CurrencyMismatch,
    ProductMismatch,
    InvalidCurrency,
    InvalidTime,
}

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid unit value: {self:?}")
    }
}

impl std::error::Error for UnitError {}

/// Exact decimal with at most 20 integer and 18 fractional digits. This
/// matches `NUMERIC(38,18)` without rounding or floating-point conversion.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Decimal(BigDecimal);

impl Decimal {
    /// Parses plain decimal notation (no exponent, separators or whitespace).
    /// Excess scale or magnitude is rejected, never truncated.
    pub fn parse(value: &str) -> Result<Self, UnitError> {
        let unsigned = value.strip_prefix('-').unwrap_or(value);
        let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
        let syntax_ok = value.len() <= 64
            && !whole.is_empty()
            && whole.bytes().all(|c| c.is_ascii_digit())
            && fraction.bytes().all(|c| c.is_ascii_digit())
            && !value.ends_with('.')
            && fraction.len() <= 18
            && whole.trim_start_matches('0').len() <= 20;
        if !syntax_ok {
            return Err(UnitError::InvalidDecimal);
        }
        let parsed = BigDecimal::from_str(value).map_err(|_| UnitError::InvalidDecimal)?;
        Ok(Self(parsed.normalized()))
    }

    /// Canonical plain decimal text suitable for a PostgreSQL NUMERIC input.
    pub fn to_numeric_text(&self) -> String {
        self.0.to_plain_string()
    }

    /// Returns true for a strictly negative value.
    pub fn is_negative(&self) -> bool {
        self.0 < 0
    }

    /// Quantizes down to a positive step. Negative inputs are rejected so
    /// callers cannot accidentally round a signed quantity toward infinity.
    pub fn floor_to_step(&self, step: &Self) -> Result<Self, UnitError> {
        if self.is_negative() || step.is_negative() {
            return Err(UnitError::Negative);
        }
        if step.0 == 0 {
            return Err(UnitError::ZeroStep);
        }
        let quotient = (&self.0 / &step.0).with_scale_round(0, RoundingMode::Down);
        let rounded = quotient * &step.0;
        Self::parse(&rounded.to_plain_string())
    }
}

impl Serialize for Decimal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_numeric_text())
    }
}

impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// ISO-like uppercase currency code. It is a denomination, not a conversion.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Currency(String);

impl Currency {
    /// Accepts 2-12 uppercase ASCII letters or digits, starting with a letter.
    pub fn new(code: &str) -> Result<Self, UnitError> {
        if (2..=12).contains(&code.len())
            && code.bytes().next().is_some_and(|c| c.is_ascii_uppercase())
            && code
                .bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            Ok(Self(code.to_owned()))
        } else {
            Err(UnitError::InvalidCurrency)
        }
    }

    /// Returns the validated code.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for Currency {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let code = String::deserialize(deserializer)?;
        Self::new(&code).map_err(serde::de::Error::custom)
    }
}

/// Signed amount in one currency; fees may be negative.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Money {
    /// Exact currency amount.
    pub amount: Decimal,
    /// Currency of the amount.
    pub currency: Currency,
}

impl Money {
    /// Adds amounts only when the currencies match; rejects overflow beyond
    /// the exact NUMERIC domain. No FX conversion or I/O occurs.
    pub fn checked_add(&self, other: &Self) -> Result<Self, UnitError> {
        if self.currency != other.currency {
            return Err(UnitError::CurrencyMismatch);
        }
        let sum = &self.amount.0 + &other.amount.0;
        Ok(Self {
            amount: Decimal::parse(&sum.to_plain_string()).map_err(|_| UnitError::OutOfRange)?,
            currency: self.currency.clone(),
        })
    }
}

/// Price in quote currency per one base currency unit; tick and specification
/// version are checked by `instrument` before planning.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Price {
    /// Nonnegative quote amount per base unit.
    amount: Decimal,
    /// Base currency.
    base: Currency,
    /// Quote currency.
    quote: Currency,
}

impl Price {
    /// Creates a nonnegative price. The product tick remains an external fact.
    pub fn new(amount: Decimal, base: Currency, quote: Currency) -> Result<Self, UnitError> {
        if amount.is_negative() {
            return Err(UnitError::Negative);
        }
        Ok(Self {
            amount,
            base,
            quote,
        })
    }

    /// Returns the exact quote amount per base unit.
    pub fn amount(&self) -> &Decimal {
        &self.amount
    }

    /// Returns the base denomination.
    pub fn base(&self) -> &Currency {
        &self.base
    }

    /// Returns the quote denomination.
    pub fn quote(&self) -> &Currency {
        &self.quote
    }
}

impl<'de> Deserialize<'de> for Price {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            amount: Decimal,
            base: Currency,
            quote: Currency,
        }
        let fields = Fields::deserialize(deserializer)?;
        Self::new(fields.amount, fields.base, fields.quote).map_err(serde::de::Error::custom)
    }
}

/// Spot base-currency units owned by one internal product.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BaseQuantity {
    /// Nonnegative base units.
    amount: Decimal,
    /// Product whose base unit this quantity represents.
    product_id: ProductId,
}

impl BaseQuantity {
    /// Creates a nonnegative spot quantity for one product.
    pub fn new(amount: Decimal, product_id: ProductId) -> Result<Self, UnitError> {
        if amount.is_negative() {
            return Err(UnitError::Negative);
        }
        Ok(Self { amount, product_id })
    }

    /// Returns exact spot base units.
    pub fn amount(&self) -> &Decimal {
        &self.amount
    }

    /// Returns the owning product identity.
    pub fn product_id(&self) -> &ProductId {
        &self.product_id
    }
}

impl<'de> Deserialize<'de> for BaseQuantity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            amount: Decimal,
            product_id: ProductId,
        }
        let fields = Fields::deserialize(deserializer)?;
        Self::new(fields.amount, fields.product_id).map_err(serde::de::Error::custom)
    }
}

/// SWAP contract count owned by one internal product; conversion to base units
/// requires a fresh contract value from `instrument`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Contracts {
    /// Nonnegative contract count.
    amount: Decimal,
    /// Product whose contracts are counted.
    product_id: ProductId,
}

impl Contracts {
    /// Creates a nonnegative contract count for one product.
    pub fn new(amount: Decimal, product_id: ProductId) -> Result<Self, UnitError> {
        if amount.is_negative() {
            return Err(UnitError::Negative);
        }
        Ok(Self { amount, product_id })
    }

    /// Returns exact contract count.
    pub fn amount(&self) -> &Decimal {
        &self.amount
    }

    /// Returns the owning product identity.
    pub fn product_id(&self) -> &ProductId {
        &self.product_id
    }
}

impl<'de> Deserialize<'de> for Contracts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            amount: Decimal,
            product_id: ProductId,
        }
        let fields = Fields::deserialize(deserializer)?;
        Self::new(fields.amount, fields.product_id).map_err(serde::de::Error::custom)
    }
}

/// Dimensionless fraction in the inclusive range 0 through 1. Basis points
/// require an explicit conversion and are not accepted as a ratio.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ratio(Decimal);

impl Ratio {
    /// Rejects negative values and values greater than one.
    pub fn new(value: Decimal) -> Result<Self, UnitError> {
        if value.is_negative() || value.0 > 1 {
            return Err(UnitError::OutOfRange);
        }
        Ok(Self(value))
    }

    /// Returns the exact dimensionless value.
    pub fn amount(&self) -> &Decimal {
        &self.0
    }
}

impl Serialize for Ratio {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Ratio {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(Decimal::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Exchange-origin UTC Unix milliseconds, kept distinct from local receipt.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct SourceTimeMs(i64);

impl SourceTimeMs {
    /// Requires nonnegative Unix milliseconds; zero may mean the epoch.
    pub fn new(value: i64) -> Result<Self, UnitError> {
        (value >= 0)
            .then_some(Self(value))
            .ok_or(UnitError::InvalidTime)
    }

    /// Returns UTC Unix milliseconds.
    pub fn get(self) -> i64 {
        self.0
    }
}

impl TryFrom<i64> for SourceTimeMs {
    type Error = UnitError;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SourceTimeMs> for i64 {
    fn from(value: SourceTimeMs) -> Self {
        value.0
    }
}

/// Local receipt UTC Unix milliseconds. It is never silently substituted for
/// exchange source time when judging data freshness.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct ReceivedTimeMs(i64);

impl ReceivedTimeMs {
    /// Requires nonnegative Unix milliseconds.
    pub fn new(value: i64) -> Result<Self, UnitError> {
        (value >= 0)
            .then_some(Self(value))
            .ok_or(UnitError::InvalidTime)
    }

    /// Returns UTC Unix milliseconds.
    pub fn get(self) -> i64 {
        self.0
    }
}

impl TryFrom<i64> for ReceivedTimeMs {
    type Error = UnitError;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ReceivedTimeMs> for i64 {
    fn from(value: ReceivedTimeMs) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_domain_and_wire_are_exact() {
        let maximum = format!("{}.{}", "9".repeat(20), "9".repeat(18));
        let decimal = Decimal::parse(&maximum).expect("maximum NUMERIC value");
        assert_eq!(decimal.to_numeric_text(), maximum);
        assert!(Decimal::parse(&format!("1{maximum}")).is_err());
        assert!(Decimal::parse("0.0000000000000000001").is_err());
        for invalid in ["NaN", "Infinity", "1e3", "+1", " 1", "1.", ""] {
            assert!(Decimal::parse(invalid).is_err());
        }
        assert!(serde_json::from_str::<Decimal>("0.1").is_err());
        assert_eq!(
            serde_json::to_string(&decimal).expect("serialize decimal"),
            format!("\"{maximum}\"")
        );
    }

    #[test]
    fn money_checks_currency_and_overflow() {
        let usd = Currency::new("USDT").expect("currency");
        let btc = Currency::new("BTC").expect("currency");
        let one = Decimal::parse("1").expect("decimal");
        let first = Money {
            amount: one.clone(),
            currency: usd.clone(),
        };
        let second = Money {
            amount: one,
            currency: btc,
        };
        assert_eq!(first.checked_add(&second), Err(UnitError::CurrencyMismatch));
        let max = Money {
            amount: Decimal::parse(&"9".repeat(20)).expect("maximum"),
            currency: usd,
        };
        assert_eq!(max.checked_add(&first), Err(UnitError::OutOfRange));
    }

    #[test]
    fn quantity_units_and_floor_boundaries() {
        let amount = Decimal::parse("1.239").expect("amount");
        let step = Decimal::parse("0.01").expect("step");
        assert_eq!(
            amount
                .floor_to_step(&step)
                .expect("floor")
                .to_numeric_text(),
            "1.23"
        );
        assert_eq!(
            step.floor_to_step(&step).expect("exact").to_numeric_text(),
            "0.01"
        );
        assert_eq!(
            Decimal::parse("0.009")
                .expect("small")
                .floor_to_step(&step)
                .expect("floor")
                .to_numeric_text(),
            "0"
        );
        assert_eq!(
            amount.floor_to_step(&Decimal::parse("0").expect("zero")),
            Err(UnitError::ZeroStep)
        );
        let product = ProductId::new("spot-a").expect("product");
        assert!(
            BaseQuantity::new(Decimal::parse("-1").expect("negative"), product.clone()).is_err()
        );
        let base = BaseQuantity::new(amount.clone(), product.clone()).expect("spot quantity");
        let contracts = Contracts::new(amount, product).expect("contract quantity");
        assert_eq!(base.amount(), contracts.amount());
        assert!(
            serde_json::from_str::<BaseQuantity>("{\"amount\":\"-1\",\"product_id\":\"spot-a\"}")
                .is_err()
        );
        assert!(
            serde_json::from_str::<Contracts>("{\"amount\":\"-1\",\"product_id\":\"swap-a\"}")
                .is_err()
        );
        assert!(
            serde_json::from_str::<Price>(
                "{\"amount\":\"-1\",\"base\":\"BTC\",\"quote\":\"USDT\"}"
            )
            .is_err()
        );
    }

    #[test]
    fn time_and_ratio_reject_bad_values() {
        assert!(SourceTimeMs::new(-1).is_err());
        assert!(ReceivedTimeMs::new(-1).is_err());
        assert!(Ratio::new(Decimal::parse("1.01").expect("ratio")).is_err());
        assert!(serde_json::from_str::<Ratio>("\"1.01\"").is_err());
        assert!(Decimal::parse(&"0".repeat(1_000)).is_err());
    }
}
