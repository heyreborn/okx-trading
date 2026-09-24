//! Strict, role-aware deployment environment parsing. Inputs are process
//! environment pairs; outputs contain addresses and secret file references,
//! never resolved secret values. Invalid input has no side effects.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use url::Url;

const PREFIX: &str = "OKX_TRADING_";
const KEYS: &[&str] = &[
    "OKX_TRADING_NODE_ID",
    "OKX_TRADING_CONFIG_PATH",
    "OKX_TRADING_ENVIRONMENT",
    "OKX_TRADING_DATABASE_URL",
    "OKX_TRADING_DATABASE_PASSWORD_FILE",
    "OKX_TRADING_KAFKA_BOOTSTRAP",
    "OKX_TRADING_OBJECT_STORE_URL",
    "OKX_TRADING_OBJECT_STORE_ACCESS_KEY_FILE",
    "OKX_TRADING_OBJECT_STORE_SECRET_KEY_FILE",
    "OKX_TRADING_OKX_API_KEY_FILE",
    "OKX_TRADING_OKX_API_SECRET_FILE",
    "OKX_TRADING_OKX_API_PASSPHRASE_FILE",
];

/// Process role used to limit deployment capabilities before adapters start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Ingest,
    Archive,
    Observe,
    Trader,
    Notifier,
    Admin,
}

/// Absolute path to a deployment-provided secret. The path is hidden in debug
/// output; reading and authorizing its contents belongs to the runner.
#[derive(Clone, Eq, PartialEq)]
pub struct SecretRef(PathBuf);

impl SecretRef {
    /// Returns the file path for runner-side resolution; this method does no I/O.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl fmt::Debug for SecretRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretRef([redacted])")
    }
}

/// Validated connection settings. URLs cannot contain embedded passwords.
#[derive(Clone, Debug)]
pub struct Deployment {
    pub role: Role,
    pub node_id: String,
    pub config_path: PathBuf,
    pub database_url: Url,
    pub database_password: SecretRef,
    pub kafka_bootstrap: Option<String>,
    pub object_store_url: Option<Url>,
    pub object_store_access_key: Option<SecretRef>,
    pub object_store_secret_key: Option<SecretRef>,
    pub okx_api_key: Option<SecretRef>,
    pub okx_api_secret: Option<SecretRef>,
    pub okx_api_passphrase: Option<SecretRef>,
}

/// Parse failure. Only field names, never supplied values, appear in errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Missing(&'static str),
    Invalid(&'static str),
    Forbidden(&'static str),
    Unknown(String),
    Duplicate(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(key) => write!(f, "missing deployment field {key}"),
            Self::Invalid(key) => write!(f, "invalid deployment field {key}"),
            Self::Forbidden(key) => write!(f, "field not permitted for role: {key}"),
            Self::Unknown(key) => write!(f, "unknown deployment field {key}"),
            Self::Duplicate(key) => write!(f, "duplicate deployment field {key}"),
        }
    }
}

impl std::error::Error for Error {}

fn required(map: &BTreeMap<String, String>, key: &'static str) -> Result<String, Error> {
    map.get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or(Error::Missing(key))
}

fn secret(map: &BTreeMap<String, String>, key: &'static str) -> Result<SecretRef, Error> {
    let path = PathBuf::from(required(map, key)?);
    if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return Err(Error::Invalid(key));
    }
    Ok(SecretRef(path))
}

fn endpoint(
    map: &BTreeMap<String, String>,
    key: &'static str,
    schemes: &[&str],
    needs_username: bool,
) -> Result<Url, Error> {
    let url = Url::parse(&required(map, key)?).map_err(|_| Error::Invalid(key))?;
    if !schemes.contains(&url.scheme())
        || url.host_str().is_none()
        || url.password().is_some()
        || (needs_username && url.username().is_empty())
        || (!needs_username && !url.username().is_empty())
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Invalid(key));
    }
    Ok(url)
}

fn permitted<T>(
    map: &BTreeMap<String, String>,
    key: &'static str,
    allowed: bool,
    parse: impl FnOnce() -> Result<T, Error>,
) -> Result<Option<T>, Error> {
    if allowed {
        Ok(Some(parse()?))
    } else if map.contains_key(key) {
        Err(Error::Forbidden(key))
    } else {
        Ok(None)
    }
}

/// Parses deployment environment pairs for a role. Unknown `OKX_TRADING_*`
/// keys, live mode, missing credentials, and extra role credentials fail closed.
/// This function performs no secret reads, network calls, or trading action.
pub fn from_pairs(
    role: Role,
    pairs: impl IntoIterator<Item = (String, String)>,
) -> Result<Deployment, Error> {
    let mut map = BTreeMap::new();
    for (key, value) in pairs {
        if key.starts_with(PREFIX) {
            if !KEYS.contains(&key.as_str()) {
                return Err(Error::Unknown(key));
            }
            if map.insert(key.clone(), value).is_some() {
                return Err(Error::Duplicate(key));
            }
        }
    }
    if required(&map, "OKX_TRADING_ENVIRONMENT")? != "demo" {
        return Err(Error::Invalid("OKX_TRADING_ENVIRONMENT"));
    }
    let node_id = required(&map, "OKX_TRADING_NODE_ID")?;
    if node_id.len() > 64
        || !node_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(Error::Invalid("OKX_TRADING_NODE_ID"));
    }
    let config_path = PathBuf::from(required(&map, "OKX_TRADING_CONFIG_PATH")?);
    if !config_path.is_absolute()
        || config_path
            .components()
            .any(|part| part == Component::ParentDir)
    {
        return Err(Error::Invalid("OKX_TRADING_CONFIG_PATH"));
    }
    let database_url = endpoint(&map, "OKX_TRADING_DATABASE_URL", &["postgresql"], true)?;
    let database_password = secret(&map, "OKX_TRADING_DATABASE_PASSWORD_FILE")?;

    let needs_kafka = matches!(
        role,
        Role::Ingest | Role::Archive | Role::Observe | Role::Trader
    );
    let kafka_bootstrap = permitted(&map, "OKX_TRADING_KAFKA_BOOTSTRAP", needs_kafka, || {
        let value = required(&map, "OKX_TRADING_KAFKA_BOOTSTRAP")?;
        let url = Url::parse(&format!("kafka://{value}"))
            .map_err(|_| Error::Invalid("OKX_TRADING_KAFKA_BOOTSTRAP"))?;
        if url.host_str().is_none()
            || url.port().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || !(url.path().is_empty() || url.path() == "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::Invalid("OKX_TRADING_KAFKA_BOOTSTRAP"));
        }
        Ok(value)
    })?;
    let needs_store = matches!(role, Role::Archive | Role::Observe | Role::Admin);
    let object_store_url = permitted(&map, "OKX_TRADING_OBJECT_STORE_URL", needs_store, || {
        endpoint(
            &map,
            "OKX_TRADING_OBJECT_STORE_URL",
            &["http", "https"],
            false,
        )
    })?;
    let object_store_access_key = permitted(
        &map,
        "OKX_TRADING_OBJECT_STORE_ACCESS_KEY_FILE",
        needs_store,
        || secret(&map, "OKX_TRADING_OBJECT_STORE_ACCESS_KEY_FILE"),
    )?;
    let object_store_secret_key = permitted(
        &map,
        "OKX_TRADING_OBJECT_STORE_SECRET_KEY_FILE",
        needs_store,
        || secret(&map, "OKX_TRADING_OBJECT_STORE_SECRET_KEY_FILE"),
    )?;
    let needs_okx = role == Role::Trader;
    let okx_api_key = permitted(&map, "OKX_TRADING_OKX_API_KEY_FILE", needs_okx, || {
        secret(&map, "OKX_TRADING_OKX_API_KEY_FILE")
    })?;
    let okx_api_secret = permitted(&map, "OKX_TRADING_OKX_API_SECRET_FILE", needs_okx, || {
        secret(&map, "OKX_TRADING_OKX_API_SECRET_FILE")
    })?;
    let okx_api_passphrase = permitted(
        &map,
        "OKX_TRADING_OKX_API_PASSPHRASE_FILE",
        needs_okx,
        || secret(&map, "OKX_TRADING_OKX_API_PASSPHRASE_FILE"),
    )?;

    Ok(Deployment {
        role,
        node_id,
        config_path,
        database_url,
        database_password,
        kafka_bootstrap,
        object_store_url,
        object_store_access_key,
        object_store_secret_key,
        okx_api_key,
        okx_api_secret,
        okx_api_passphrase,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Vec<(String, String)> {
        [
            ("OKX_TRADING_ENVIRONMENT", "demo"),
            ("OKX_TRADING_NODE_ID", "node-1"),
            ("OKX_TRADING_CONFIG_PATH", "/config/policy.toml"),
            (
                "OKX_TRADING_DATABASE_URL",
                "postgresql://okx_dev@postgres:5432/okx",
            ),
            ("OKX_TRADING_DATABASE_PASSWORD_FILE", "/run/secrets/pg"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
    }

    fn add(pairs: &mut Vec<(String, String)>, key: &str, value: &str) {
        pairs.push((key.to_owned(), value.to_owned()));
    }

    #[test]
    fn ingest_has_only_required_connections() {
        let mut pairs = base();
        add(&mut pairs, "OKX_TRADING_KAFKA_BOOTSTRAP", "kafka:9092");
        let config = from_pairs(Role::Ingest, pairs).expect("valid ingest deployment");
        assert_eq!(config.kafka_bootstrap.as_deref(), Some("kafka:9092"));
        assert!(config.okx_api_key.is_none());
        assert!(config.object_store_url.is_none());
    }

    #[test]
    fn trader_requires_own_secret_refs() {
        let mut pairs = base();
        add(&mut pairs, "OKX_TRADING_KAFKA_BOOTSTRAP", "kafka:9092");
        assert_eq!(
            from_pairs(Role::Trader, pairs.clone()).expect_err("missing key must fail"),
            Error::Missing("OKX_TRADING_OKX_API_KEY_FILE")
        );
        add(
            &mut pairs,
            "OKX_TRADING_OKX_API_KEY_FILE",
            "/run/secrets/key",
        );
        add(
            &mut pairs,
            "OKX_TRADING_OKX_API_SECRET_FILE",
            "/run/secrets/secret",
        );
        add(
            &mut pairs,
            "OKX_TRADING_OKX_API_PASSPHRASE_FILE",
            "/run/secrets/passphrase",
        );
        let parsed = from_pairs(Role::Trader, pairs).expect("valid trader wiring");
        assert!(!format!("{parsed:?}").contains("/run/secrets/secret"));
    }

    #[test]
    fn rejects_environment_overrides_and_mixed_roles() {
        let mut pairs = base();
        add(&mut pairs, "OKX_TRADING_ENABLE_TRADING", "true");
        assert!(matches!(
            from_pairs(Role::Admin, pairs),
            Err(Error::Unknown(_))
        ));

        let mut pairs = base();
        pairs[0].1 = "live".to_owned();
        assert_eq!(
            from_pairs(Role::Admin, pairs).expect_err("live mode must fail"),
            Error::Invalid("OKX_TRADING_ENVIRONMENT")
        );

        let mut pairs = base();
        add(
            &mut pairs,
            "OKX_TRADING_OKX_API_KEY_FILE",
            "/run/secrets/key",
        );
        assert_eq!(
            from_pairs(Role::Notifier, pairs).expect_err("extra credential must fail"),
            Error::Forbidden("OKX_TRADING_OKX_API_KEY_FILE")
        );
    }

    #[test]
    fn rejects_inline_passwords_and_relative_secret_paths_without_leaking_values() {
        let mut pairs = base();
        pairs[3].1 = "postgresql://postgres:5432/okx".to_owned();
        assert_eq!(
            from_pairs(Role::Notifier, pairs).expect_err("database user must be explicit"),
            Error::Invalid("OKX_TRADING_DATABASE_URL")
        );

        let mut pairs = base();
        pairs[3].1 = "postgresql://user:private-value@postgres:5432/okx".to_owned();
        let error = from_pairs(Role::Notifier, pairs).expect_err("inline password must fail");
        assert_eq!(error, Error::Invalid("OKX_TRADING_DATABASE_URL"));
        assert!(!error.to_string().contains("private-value"));

        let mut pairs = base();
        pairs[4].1 = "relative-secret".to_owned();
        assert_eq!(
            from_pairs(Role::Notifier, pairs).expect_err("relative path must fail"),
            Error::Invalid("OKX_TRADING_DATABASE_PASSWORD_FILE")
        );

        let mut pairs = base();
        add(
            &mut pairs,
            "OKX_TRADING_KAFKA_BOOTSTRAP",
            "user:private-value@kafka:9092",
        );
        let error = from_pairs(Role::Ingest, pairs).expect_err("inline password must fail");
        assert_eq!(error, Error::Invalid("OKX_TRADING_KAFKA_BOOTSTRAP"));
        assert!(!error.to_string().contains("private-value"));
    }
}
