//! Distinct stable identities for local objects and the OKX product namespace.
//! Inputs are bounded ASCII strings; IDs carry no authorization or freshness.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// Invalid identity input. The supplied value is intentionally omitted from errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidId;

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid identity")
    }
}

impl std::error::Error for InvalidId {}

fn valid(value: &str) -> bool {
    value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b':'))
}

macro_rules! identity {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// Validates a stable, nonempty ASCII identifier of at most 128 bytes.
            /// The value is not normalized; invalid input returns `InvalidId`.
            pub fn new(value: impl Into<String>) -> Result<Self, InvalidId> {
                let value = value.into();
                valid(&value).then_some(Self(value)).ok_or(InvalidId)
            }

            /// Returns the validated identifier without changing its namespace.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = InvalidId;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

identity!(
    AccountId,
    "Local account identity; never substitutes for an OKX account fact."
);
identity!(
    ProductId,
    "Internal product identity; never substitutes for an OKX `instId`."
);
identity!(
    OkxInstrumentId,
    "OKX `instId` within the separately checked demo environment."
);
identity!(StrategyTypeId, "Static strategy implementation identity.");
identity!(
    StrategyInstanceId,
    "Unique strategy instance and inventory owner identity."
);
identity!(BindingId, "Stable input or target binding identity.");
identity!(FeedId, "Stable market feed identity.");
identity!(
    EventId,
    "Canonical market event identity within its schema namespace."
);
identity!(TargetId, "Stable strategy target identity.");
identity!(DecisionId, "Stable local decision identity.");
identity!(OrderIntentId, "Stable local order intent identity.");
identity!(
    ClientOrderId,
    "Stable OKX client order identity; callers must never reuse it."
);
identity!(ExchangeOrderId, "OKX assigned order identity.");
identity!(
    FillId,
    "OKX assigned fill identity within its order namespace."
);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn identities_round_trip_as_strings_and_reject_invalid_wire_values() {
        let product = ProductId::new("spot-btc-usdt").expect("valid product ID");
        let wire = serde_json::to_string(&product).expect("serialize product ID");
        assert_eq!(wire, "\"spot-btc-usdt\"");
        assert_eq!(
            serde_json::from_str::<ProductId>(&wire).expect("deserialize product ID"),
            product
        );
        for bad in ["\"\"", "\"-bad\"", "\"a b\"", "\"a\\nb\"", "123", "null"] {
            assert!(serde_json::from_str::<ProductId>(bad).is_err());
        }
        assert!(ProductId::new("a".repeat(129)).is_err());
        assert!(ProductId::new("é").is_err());
    }

    #[test]
    fn duplicate_ids_keep_equality_without_collapsing_namespaces() {
        let first = StrategyInstanceId::new("alpha-1").expect("valid instance ID");
        let second = StrategyInstanceId::new("alpha-1").expect("valid duplicate ID");
        let mut unique = HashSet::new();
        assert!(unique.insert(first));
        assert!(!unique.insert(second));
        assert_eq!(
            StrategyTypeId::new("alpha-1")
                .expect("valid type ID")
                .as_str(),
            "alpha-1"
        );
    }

    #[test]
    fn versioned_envelope_can_preserve_identity_across_versions() {
        #[derive(Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Envelope {
            version: u16,
            event_id: EventId,
        }
        let decoded: Envelope = serde_json::from_str("{\"version\":1,\"event_id\":\"event:1\"}")
            .expect("valid envelope");
        assert_eq!(decoded.event_id.as_str(), "event:1");
        assert_eq!(decoded.version, 1);
        assert!(
            serde_json::from_str::<Envelope>(
                "{\"version\":1,\"event_id\":\"event:1\",\"extra\":true}"
            )
            .is_err()
        );
    }
}
