//! Read-only OKX demo REST adapter. Only fixed GET endpoints are exposed;
//! HTTP status, business code, clock, size and typed response checks fail closed.
//! See the [package README](../README.md).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use instrument::{AccountEligibility, FeeSchedule, InstrumentSpec, TradingState};
use model::facts::PositionMode;
use model::facts::ProductKind;
use model::identity::{AccountId, OkxInstrumentId};
use model::units::{Currency, Decimal, ReceivedTimeMs, SourceTimeMs};
use reqwest::{Client as HttpClient, StatusCode, Url, redirect::Policy};
use serde::Deserialize;
use sha2::Sha256;
use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use time::{OffsetDateTime, macros::format_description};
use tokio::sync::Mutex as AsyncMutex;

const MAX_BODY_BYTES: usize = 1024 * 1024;
const CLOCK_MAX_AGE: Duration = Duration::from_secs(60);
const CLOCK_MAX_SKEW_MS: i64 = 5_000;
const REQUEST_INTERVAL: Duration = Duration::from_millis(500);
const TIME_INTERVAL: Duration = Duration::from_secs(5);

/// Sanitized read-only API error. Response text and credentials are never
/// included in diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Transport,
    HttpStatus(u16),
    ApiCode(String),
    InvalidResponse,
    ResponseTooLarge,
    ClockUnavailable,
    ClockSkew,
    InvalidCredentials,
    InvalidEndpoint,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OKX read-only request failed: {self:?}")
    }
}

impl std::error::Error for Error {}

/// Demo API credentials supplied by a runner after resolving secret refs.
/// `Debug` and errors never expose their values.
pub struct Credentials {
    key: String,
    secret: String,
    passphrase: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credentials([redacted])")
    }
}

impl Credentials {
    /// Requires nonempty credential fields. No I/O or permission check occurs.
    pub fn new(key: String, secret: String, passphrase: String) -> Result<Self, Error> {
        if key.is_empty()
            || secret.is_empty()
            || passphrase.is_empty()
            || [key.as_str(), secret.as_str(), passphrase.as_str()]
                .iter()
                .any(|v| v.contains('\r') || v.contains('\n'))
        {
            return Err(Error::InvalidCredentials);
        }
        Ok(Self {
            key,
            secret,
            passphrase,
        })
    }
}

/// Supported demo REST regions. The deployment must select the region of its
/// actual demo account; this adapter never redirects to another host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    /// Global account endpoint.
    Global,
    /// US account endpoint.
    UnitedStates,
}

impl Region {
    fn base(self) -> &'static str {
        match self {
            Self::Global => "https://openapi.okx.com/",
            Self::UnitedStates => "https://us.okx.com/",
        }
    }
}

/// Supported product category for directory GET parameters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstrumentType {
    /// SPOT instruments.
    Spot,
    /// Perpetual SWAP instruments; non-linear records are rejected below.
    Swap,
}

impl InstrumentType {
    fn as_okx(self) -> &'static str {
        match self {
            Self::Spot => "SPOT",
            Self::Swap => "SWAP",
        }
    }
}

/// Read-only parsed public instrument fields. Later `instrument` rules compare
/// these against immutable local mappings and freshness limits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicInstrument {
    /// OKX instrument ID.
    pub inst_id: OkxInstrumentId,
    /// Requested product category.
    pub inst_type: InstrumentType,
    /// Current trading state: `live`, `post_only`, or unsupported.
    pub state: InstrumentState,
    /// Base denomination, from SPOT `baseCcy` or linear SWAP `ctValCcy`.
    pub base: Currency,
    /// Quote denomination, from SPOT `quoteCcy` or SWAP `settleCcy`.
    pub quote: Currency,
    /// Quote price tick, exact decimal string.
    pub tick_size: Decimal,
    /// Base units for SPOT, contracts for SWAP.
    pub lot_size: Decimal,
    /// Base units for SPOT, contracts for SWAP.
    pub min_size: Decimal,
    /// Contract type for SWAP; must be `linear`.
    pub contract_type: Option<String>,
    /// Contract value for SWAP.
    pub contract_value: Option<Decimal>,
    /// Currency of contract value for SWAP.
    pub contract_value_currency: Option<Currency>,
    /// Settlement currency for SWAP.
    pub settlement_currency: Option<Currency>,
    /// Fee group ID; needed to select current fee rates.
    pub fee_group_id: Option<String>,
}

/// Strictly recognized exchange trading state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstrumentState {
    Live,
    PostOnly,
    Inactive,
}

/// Actual account config. `account_level` is preserved for later capability
/// checks; `position_mode` is the exchange-observed mode, never a local guess.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountConfiguration {
    /// Raw documented account level code.
    pub account_level: String,
    /// Actual net or dual-side mode.
    pub position_mode: PositionMode,
}

/// One account's available product IDs from a read-only API query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountInstruments {
    /// Local account identity associated with the supplied credentials.
    pub account_id: AccountId,
    /// Requested product category.
    pub inst_type: InstrumentType,
    /// Exchange instrument IDs returned for that account.
    pub inst_ids: Vec<OkxInstrumentId>,
}

/// One read-only response with server-clock-derived observation time and local
/// receipt time. These do not claim an exchange event update timestamp.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation<T> {
    /// Typed response value.
    pub value: T,
    /// Estimated exchange UTC Unix milliseconds at observation.
    pub source_time_ms: SourceTimeMs,
    /// Local receipt UTC Unix milliseconds.
    pub received_time_ms: ReceivedTimeMs,
}

/// Fee group rates returned by the account API. Fees retain the exchange's
/// sign convention and require separate fee-currency checks at execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeRates {
    /// The requested fee group.
    pub group_id: String,
    /// Signed maker fraction.
    pub maker: Decimal,
    /// Signed taker fraction.
    pub taker: Decimal,
    /// Exchange-reported last fee-data update time in UTC Unix milliseconds.
    pub updated_time_ms: SourceTimeMs,
}

impl Observation<Vec<PublicInstrument>> {
    /// Converts validated public rows into versioned, read-only product specs.
    /// A local mapping and account gate must still be checked by `instrument`.
    pub fn to_instrument_specs(&self) -> Result<Vec<InstrumentSpec>, Error> {
        self.value.iter().map(|row| {
            let version = digest(&serde_json::json!({
                "inst_id": row.inst_id.as_str(), "kind": row.inst_type.as_okx(),
                "state": format!("{:?}", row.state), "base": row.base.as_str(),
                "quote": row.quote.as_str(), "tick": row.tick_size.to_numeric_text(),
                "lot": row.lot_size.to_numeric_text(), "min": row.min_size.to_numeric_text(),
                "contract_value": row.contract_value.as_ref().map(Decimal::to_numeric_text),
                "contract_value_currency": row.contract_value_currency.as_ref().map(Currency::as_str),
                "settlement": row.settlement_currency.as_ref().map(Currency::as_str),
                "fee_group_id": row.fee_group_id,
            }))?;
            Ok(InstrumentSpec {
                okx_inst_id: row.inst_id.clone(),
                kind: match row.inst_type { InstrumentType::Spot => ProductKind::Spot, InstrumentType::Swap => ProductKind::LinearUsdtSwap },
                base: row.base.clone(), quote: row.quote.clone(), settlement: row.settlement_currency.clone(),
                version,
                state: match row.state { InstrumentState::Live => TradingState::Live, InstrumentState::PostOnly => TradingState::PostOnly, InstrumentState::Inactive => TradingState::Inactive },
                tick_size: row.tick_size.clone(), lot_size: row.lot_size.clone(), min_size: row.min_size.clone(),
                contract_value: row.contract_value.clone(), contract_value_currency: row.contract_value_currency.clone(),
                fee_group_id: row.fee_group_id.clone(),
                source_time_ms: self.source_time_ms, received_time_ms: self.received_time_ms,
            })
        }).collect()
    }
}

impl Observation<PublicInstrument> {
    /// Converts one allowlisted product response into a versioned spec.
    pub fn to_instrument_spec(&self) -> Result<InstrumentSpec, Error> {
        let batch = Observation {
            value: vec![self.value.clone()],
            source_time_ms: self.source_time_ms,
            received_time_ms: self.received_time_ms,
        };
        one(batch.to_instrument_specs()?)
    }
}

impl Observation<AccountInstruments> {
    /// Combines two observations from the same authenticated client. The older
    /// source time governs freshness; later receipt time governs age checks.
    pub fn to_eligibility(&self, config: &Observation<AccountConfiguration>) -> AccountEligibility {
        AccountEligibility {
            account_id: self.value.account_id.clone(),
            position_mode: config.value.position_mode,
            instrument_ids: self.value.inst_ids.clone(),
            source_time_ms: self.source_time_ms.min(config.source_time_ms),
            received_time_ms: self.received_time_ms.max(config.received_time_ms),
        }
    }
}

impl Observation<FeeRates> {
    /// Associates the selected fee group with a verified product identity.
    /// The caller must compare this instrument ID and freshness in `Directory`.
    pub fn to_fee_schedule(&self, inst_id: OkxInstrumentId) -> Result<FeeSchedule, Error> {
        let version = digest(&serde_json::json!({
            "inst_id": inst_id.as_str(), "group_id": self.value.group_id,
            "maker": self.value.maker.to_numeric_text(), "taker": self.value.taker.to_numeric_text(),
            "updated_time_ms": self.value.updated_time_ms.get(),
        }))?;
        Ok(FeeSchedule {
            okx_inst_id: inst_id,
            group_id: self.value.group_id.clone(),
            version,
            maker: self.value.maker.clone(),
            taker: self.value.taker.clone(),
            source_time_ms: self.source_time_ms,
            received_time_ms: self.received_time_ms,
        })
    }
}

fn digest(value: &serde_json::Value) -> Result<String, Error> {
    let bytes = serde_json::to_vec(value).map_err(|_| Error::InvalidResponse)?;
    use sha2::Digest;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[derive(Clone, Copy)]
struct ClockSync {
    at: Instant,
    server_ms: i64,
    local_ms: i64,
}

/// Reusable GET-only client with conservative per-client rate limiting. The
/// runner must not create multiple clients to bypass per-user/IP limits.
pub struct ReadOnlyClient {
    http: HttpClient,
    base: Url,
    credentials: Credentials,
    last_request: AsyncMutex<Option<Instant>>,
    last_time_request: AsyncMutex<Option<Instant>>,
    clock: Mutex<Option<ClockSync>>,
}

impl ReadOnlyClient {
    /// Creates a demo-only client for a known regional endpoint. Redirects and
    /// ambient HTTP proxies are disabled; no request is sent at construction.
    pub fn new_demo(region: Region, credentials: Credentials) -> Result<Self, Error> {
        Self::build(region.base(), credentials)
    }

    fn build(base: &str, credentials: Credentials) -> Result<Self, Error> {
        let base = Url::parse(base).map_err(|_| Error::InvalidEndpoint)?;
        let http = HttpClient::builder()
            .redirect(Policy::none())
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| Error::Transport)?;
        Ok(Self {
            http,
            base,
            credentials,
            last_request: AsyncMutex::new(None),
            last_time_request: AsyncMutex::new(None),
            clock: Mutex::new(None),
        })
    }

    /// Reads OKX system time and checks the local clock within five seconds.
    /// The endpoint is limited to one call per five seconds per client.
    pub async fn synchronize_clock(&self) -> Result<i64, Error> {
        let mut last = self.last_time_request.lock().await;
        if let Some(previous) = *last {
            tokio::time::sleep(TIME_INTERVAL.saturating_sub(previous.elapsed())).await;
        }
        *last = Some(Instant::now());
        drop(last);
        let before = unix_ms()?;
        let rows: Vec<TimeRow> = self.get_rows("/api/v5/public/time", false).await?;
        let after = unix_ms()?;
        let row = one(rows)?;
        let server_ms = row.ts.parse::<i64>().map_err(|_| Error::InvalidResponse)?;
        let midpoint = before
            .checked_add((after - before) / 2)
            .ok_or(Error::InvalidResponse)?;
        if (server_ms - midpoint).abs() > CLOCK_MAX_SKEW_MS {
            return Err(Error::ClockSkew);
        }
        let mut clock = self.clock.lock().map_err(|_| Error::ClockUnavailable)?;
        *clock = Some(ClockSync {
            at: Instant::now(),
            server_ms,
            local_ms: midpoint,
        });
        Ok(server_ms)
    }

    /// Reads a public SPOT or SWAP directory. A recent clock sync is required
    /// so later callers can timestamp the observation honestly.
    pub async fn public_instruments(
        &self,
        inst_type: InstrumentType,
    ) -> Result<Observation<Vec<PublicInstrument>>, Error> {
        self.require_clock()?;
        let path = format!("/api/v5/public/instruments?instType={}", inst_type.as_okx());
        let rows: Vec<InstrumentRow> = self.get_rows(&path, false).await?;
        let values = rows
            .into_iter()
            .map(|row| row.parse(inst_type))
            .collect::<Result<Vec<_>, _>>()?;
        self.observation(values)
    }

    /// Reads one allowlisted public instrument by `instId`, avoiding a large
    /// full-directory response. The returned ID must exactly match the query.
    pub async fn public_instrument(
        &self,
        inst_type: InstrumentType,
        inst_id: &OkxInstrumentId,
    ) -> Result<Observation<PublicInstrument>, Error> {
        self.require_clock()?;
        let path = format!(
            "/api/v5/public/instruments?instType={}&instId={}",
            inst_type.as_okx(),
            inst_id.as_str()
        );
        let rows: Vec<InstrumentRow> = self.get_rows(&path, false).await?;
        let row = one(rows)?.parse(inst_type)?;
        if &row.inst_id != inst_id {
            return Err(Error::InvalidResponse);
        }
        self.observation(row)
    }

    /// Reads actual account level and position mode. Requires demo credentials
    /// with Read permission and a recent successful clock check.
    pub async fn account_configuration(&self) -> Result<Observation<AccountConfiguration>, Error> {
        self.require_clock()?;
        let rows: Vec<AccountRow> = self.get_rows("/api/v5/account/config", true).await?;
        let row = one(rows)?;
        let position_mode = match row.pos_mode.as_str() {
            "net_mode" => PositionMode::NetMode,
            "long_short_mode" => PositionMode::LongShortMode,
            _ => return Err(Error::InvalidResponse),
        };
        if !matches!(row.acct_lv.as_str(), "2" | "3" | "4") {
            return Err(Error::InvalidResponse);
        }
        self.observation(AccountConfiguration {
            account_level: row.acct_lv,
            position_mode,
        })
    }

    /// Reads account-visible IDs for one product category. Returning an ID is
    /// only a fact, not local permission or order approval.
    pub async fn account_instruments(
        &self,
        account_id: AccountId,
        inst_type: InstrumentType,
    ) -> Result<Observation<AccountInstruments>, Error> {
        self.require_clock()?;
        let path = format!(
            "/api/v5/account/instruments?instType={}",
            inst_type.as_okx()
        );
        let rows: Vec<AccountInstrumentRow> = self.get_rows(&path, true).await?;
        let mut ids = Vec::with_capacity(rows.len());
        let mut seen = std::collections::HashSet::new();
        for row in rows {
            if row.inst_type != inst_type.as_okx() {
                return Err(Error::InvalidResponse);
            }
            let id = OkxInstrumentId::new(row.inst_id).map_err(|_| Error::InvalidResponse)?;
            if !seen.insert(id.clone()) {
                return Err(Error::InvalidResponse);
            }
            ids.push(id);
        }
        self.observation(AccountInstruments {
            account_id,
            inst_type,
            inst_ids: ids,
        })
    }

    /// Reads one current fee group for a product category. The group ID must
    /// come from a fresh instrument directory; no default fee is assumed.
    pub async fn fee_group(
        &self,
        inst_type: InstrumentType,
        group_id: &str,
    ) -> Result<Observation<FeeRates>, Error> {
        self.require_clock()?;
        if group_id.is_empty()
            || group_id.len() > 32
            || !group_id.bytes().all(|c| c.is_ascii_alphanumeric())
        {
            return Err(Error::InvalidResponse);
        }
        let path = format!(
            "/api/v5/account/trade-fee?instType={}&groupId={group_id}",
            inst_type.as_okx()
        );
        let rows: Vec<FeeRow> = self.get_rows(&path, true).await?;
        let row = one(rows)?;
        if row.inst_type != inst_type.as_okx() {
            return Err(Error::InvalidResponse);
        }
        let group = one(row
            .fee_group
            .into_iter()
            .filter(|g| g.group_id == group_id)
            .collect())?;
        let maker = Decimal::parse(&group.maker).map_err(|_| Error::InvalidResponse)?;
        let taker = Decimal::parse(&group.taker).map_err(|_| Error::InvalidResponse)?;
        let lower = Decimal::parse("-1").map_err(|_| Error::InvalidResponse)?;
        let upper = Decimal::parse("1").map_err(|_| Error::InvalidResponse)?;
        if maker < lower || maker > upper || taker < lower || taker > upper {
            return Err(Error::InvalidResponse);
        }
        let fee_source =
            SourceTimeMs::new(row.ts.parse::<i64>().map_err(|_| Error::InvalidResponse)?)
                .map_err(|_| Error::InvalidResponse)?;
        let observed = self.observation(FeeRates {
            group_id: group_id.to_owned(),
            maker,
            taker,
            updated_time_ms: fee_source,
        })?;
        if fee_source.get() - observed.source_time_ms.get() > CLOCK_MAX_SKEW_MS {
            return Err(Error::InvalidResponse);
        }
        Ok(observed)
    }

    fn observation<T>(&self, value: T) -> Result<Observation<T>, Error> {
        let clock = self.clock.lock().map_err(|_| Error::ClockUnavailable)?;
        let sync = clock.as_ref().ok_or(Error::ClockUnavailable)?;
        if sync.at.elapsed() > CLOCK_MAX_AGE {
            return Err(Error::ClockUnavailable);
        }
        let local_ms = unix_ms()?;
        let exchange_ms = local_ms
            .checked_add(sync.server_ms - sync.local_ms)
            .ok_or(Error::ClockUnavailable)?;
        let source_time_ms =
            SourceTimeMs::new(exchange_ms.min(local_ms)).map_err(|_| Error::ClockUnavailable)?;
        let received_time_ms =
            ReceivedTimeMs::new(local_ms).map_err(|_| Error::ClockUnavailable)?;
        Ok(Observation {
            value,
            source_time_ms,
            received_time_ms,
        })
    }

    fn require_clock(&self) -> Result<(), Error> {
        let clock = self.clock.lock().map_err(|_| Error::ClockUnavailable)?;
        if clock
            .as_ref()
            .is_none_or(|v| v.at.elapsed() > CLOCK_MAX_AGE)
        {
            return Err(Error::ClockUnavailable);
        }
        Ok(())
    }

    fn timestamp(&self) -> Result<String, Error> {
        let clock = self.clock.lock().map_err(|_| Error::ClockUnavailable)?;
        let sync = clock.as_ref().ok_or(Error::ClockUnavailable)?;
        if sync.at.elapsed() > CLOCK_MAX_AGE {
            return Err(Error::ClockUnavailable);
        }
        let now = unix_ms()?;
        let elapsed_ms =
            i64::try_from(sync.at.elapsed().as_millis()).map_err(|_| Error::ClockUnavailable)?;
        if (now - sync.local_ms - elapsed_ms).abs() > 2_000 {
            return Err(Error::ClockSkew);
        }
        let adjusted = now
            .checked_add(sync.server_ms - sync.local_ms)
            .ok_or(Error::ClockUnavailable)?;
        let nanos = i128::from(adjusted) * 1_000_000;
        let utc = OffsetDateTime::from_unix_timestamp_nanos(nanos)
            .map_err(|_| Error::ClockUnavailable)?;
        let format = format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        utc.format(&format).map_err(|_| Error::ClockUnavailable)
    }

    async fn get_rows<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        private: bool,
    ) -> Result<Vec<T>, Error> {
        let mut last = self.last_request.lock().await;
        if let Some(previous) = *last {
            tokio::time::sleep(REQUEST_INTERVAL.saturating_sub(previous.elapsed())).await;
        }
        *last = Some(Instant::now());
        drop(last);
        let url = self.base.join(path).map_err(|_| Error::InvalidEndpoint)?;
        if url.host_str() != self.base.host_str() || url.scheme() != self.base.scheme() {
            return Err(Error::InvalidEndpoint);
        }
        let mut request = self.http.get(url).header("x-simulated-trading", "1");
        if private {
            let timestamp = self.timestamp()?;
            let signature = sign(&self.credentials.secret, &timestamp, path)?;
            request = request
                .header("OK-ACCESS-KEY", &self.credentials.key)
                .header("OK-ACCESS-SIGN", signature)
                .header("OK-ACCESS-TIMESTAMP", timestamp)
                .header("OK-ACCESS-PASSPHRASE", &self.credentials.passphrase);
        }
        let mut response = request.send().await.map_err(|_| Error::Transport)?;
        if response.status() != StatusCode::OK {
            return Err(Error::HttpStatus(response.status().as_u16()));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if body
                .len()
                .checked_add(chunk.len())
                .is_none_or(|size| size > MAX_BODY_BYTES)
            {
                return Err(Error::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        let envelope: ApiEnvelope<serde_json::Value> =
            serde_json::from_slice(&body).map_err(|_| Error::InvalidResponse)?;
        if envelope.code != "0" {
            if !envelope.code.is_empty()
                && envelope.code.len() <= 10
                && envelope.code.bytes().all(|v| v.is_ascii_digit())
            {
                return Err(Error::ApiCode(envelope.code));
            }
            return Err(Error::InvalidResponse);
        }
        envelope
            .data
            .into_iter()
            .map(|row| serde_json::from_value(row).map_err(|_| Error::InvalidResponse))
            .collect()
    }
}

fn unix_ms() -> Result<i64, Error> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::ClockUnavailable)?;
    i64::try_from(elapsed.as_millis()).map_err(|_| Error::ClockUnavailable)
}

fn one<T>(mut rows: Vec<T>) -> Result<T, Error> {
    if rows.len() != 1 {
        return Err(Error::InvalidResponse);
    }
    rows.pop().ok_or(Error::InvalidResponse)
}

fn sign(secret: &str, timestamp: &str, path: &str) -> Result<String, Error> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| Error::InvalidCredentials)?;
    mac.update(timestamp.as_bytes());
    mac.update(b"GET");
    mac.update(path.as_bytes());
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}

#[derive(Deserialize)]
struct ApiEnvelope<T> {
    code: String,
    data: Vec<T>,
}

#[derive(Deserialize)]
struct TimeRow {
    ts: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountRow {
    acct_lv: String,
    pos_mode: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountInstrumentRow {
    inst_id: String,
    inst_type: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeeRow {
    inst_type: String,
    ts: String,
    fee_group: Vec<FeeGroupRow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeeGroupRow {
    group_id: String,
    maker: String,
    taker: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstrumentRow {
    inst_id: String,
    inst_type: String,
    state: String,
    #[serde(default)]
    base_ccy: String,
    #[serde(default)]
    quote_ccy: String,
    tick_sz: String,
    lot_sz: String,
    min_sz: String,
    #[serde(default)]
    ct_type: String,
    #[serde(default)]
    ct_val: String,
    #[serde(default)]
    ct_val_ccy: String,
    #[serde(default)]
    settle_ccy: String,
    #[serde(default)]
    group_id: String,
}

impl InstrumentRow {
    fn parse(self, requested: InstrumentType) -> Result<PublicInstrument, Error> {
        if self.inst_type != requested.as_okx() {
            return Err(Error::InvalidResponse);
        }
        let inst_id = OkxInstrumentId::new(self.inst_id).map_err(|_| Error::InvalidResponse)?;
        let state = match self.state.as_str() {
            "live" => InstrumentState::Live,
            "post_only" => InstrumentState::PostOnly,
            "suspend" | "rebase" => InstrumentState::Inactive,
            _ => return Err(Error::InvalidResponse),
        };
        let tick_size = positive(&self.tick_sz)?;
        let lot_size = positive(&self.lot_sz)?;
        let min_size = positive(&self.min_sz)?;
        let (base, quote) = match requested {
            InstrumentType::Spot => (
                Currency::new(&self.base_ccy).map_err(|_| Error::InvalidResponse)?,
                Currency::new(&self.quote_ccy).map_err(|_| Error::InvalidResponse)?,
            ),
            InstrumentType::Swap => (
                Currency::new(&self.ct_val_ccy).map_err(|_| Error::InvalidResponse)?,
                Currency::new(&self.settle_ccy).map_err(|_| Error::InvalidResponse)?,
            ),
        };
        if requested == InstrumentType::Swap {
            if self.ct_type != "linear" || self.settle_ccy != "USDT" || self.ct_val_ccy.is_empty() {
                return Err(Error::InvalidResponse);
            }
            positive(&self.ct_val)?;
        }
        Ok(PublicInstrument {
            inst_id,
            inst_type: requested,
            state,
            base,
            quote,
            tick_size,
            lot_size,
            min_size,
            contract_type: (!self.ct_type.is_empty()).then_some(self.ct_type),
            contract_value: if self.ct_val.is_empty() {
                None
            } else {
                Some(positive(&self.ct_val)?)
            },
            contract_value_currency: if self.ct_val_ccy.is_empty() {
                None
            } else {
                Some(Currency::new(&self.ct_val_ccy).map_err(|_| Error::InvalidResponse)?)
            },
            settlement_currency: if self.settle_ccy.is_empty() {
                None
            } else {
                Some(Currency::new(&self.settle_ccy).map_err(|_| Error::InvalidResponse)?)
            },
            fee_group_id: (!self.group_id.is_empty()).then_some(self.group_id),
        })
    }
}

fn positive(value: &str) -> Result<Decimal, Error> {
    let parsed = Decimal::parse(value).map_err(|_| Error::InvalidResponse)?;
    if parsed.is_negative() || parsed.to_numeric_text() == "0" {
        return Err(Error::InvalidResponse);
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds() -> Credentials {
        Credentials::new("test-key".into(), "test-secret".into(), "test-pass".into())
            .expect("test credentials")
    }

    #[test]
    fn signature_includes_query_and_credentials_are_redacted() {
        let signature = sign(
            "secret",
            "2020-12-08T09:08:57.715Z",
            "/api/v5/account/instruments?instType=SPOT",
        )
        .expect("signature");
        assert_eq!(signature, "XlN4DYRJrQwfGHmKTtL3sOyyUQhHPuYbLFciLuRUfxg=");
        assert_ne!(
            signature,
            sign(
                "secret",
                "2020-12-08T09:08:57.715Z",
                "/api/v5/account/instruments"
            )
            .expect("signature")
        );
        assert_eq!(format!("{:?}", creds()), "Credentials([redacted])");
    }

    #[tokio::test]
    async fn mock_demo_get_checks_headers_code_and_typed_fields() {
        let mut server = mockito::Server::new_async().await;
        let time = unix_ms().expect("local time").to_string();
        let clock = server
            .mock("GET", "/api/v5/public/time")
            .match_header("x-simulated-trading", "1")
            .with_status(200)
            .with_body(format!(
                "{{\"code\":\"0\",\"data\":[{{\"ts\":\"{time}\"}}]}}"
            ))
            .create_async()
            .await;
        let account = server
            .mock("GET", "/api/v5/account/config")
            .match_header("x-simulated-trading", "1")
            .match_header("ok-access-key", "test-key")
            .with_status(200)
            .with_body(
                "{\"code\":\"0\",\"data\":[{\"acctLv\":\"2\",\"posMode\":\"long_short_mode\"}]}",
            )
            .create_async()
            .await;
        let client = ReadOnlyClient::build(&server.url(), creds()).expect("client");
        client.synchronize_clock().await.expect("clock");
        let actual = client
            .account_configuration()
            .await
            .expect("account config");
        assert_eq!(actual.value.position_mode, PositionMode::LongShortMode);
        clock.assert_async().await;
        account.assert_async().await;
    }

    #[tokio::test]
    async fn mock_rejects_http_business_and_unknown_state() {
        let mut server = mockito::Server::new_async().await;
        let http_error = server
            .mock("GET", "/api/v5/public/time")
            .with_status(429)
            .create_async()
            .await;
        let client = ReadOnlyClient::build(&server.url(), creds()).expect("client");
        assert_eq!(
            client.synchronize_clock().await,
            Err(Error::HttpStatus(429))
        );
        http_error.assert_async().await;
        let business = server
            .mock("GET", "/api/v5/public/time")
            .with_status(200)
            .with_body("{\"code\":\"50011\",\"data\":[],\"msg\":\"secret token\"}")
            .create_async()
            .await;
        assert_eq!(
            client
                .get_rows::<TimeRow>("/api/v5/public/time", false)
                .await
                .err(),
            Some(Error::ApiCode("50011".into()))
        );
        business.assert_async().await;
        let row: InstrumentRow = serde_json::from_str("{\"instId\":\"BTC-USDT\",\"instType\":\"SPOT\",\"state\":\"unknown\",\"tickSz\":\"0.1\",\"lotSz\":\"0.001\",\"minSz\":\"0.001\"}").expect("row");
        assert_eq!(row.parse(InstrumentType::Spot), Err(Error::InvalidResponse));
    }

    #[tokio::test]
    async fn mock_directory_and_fee_group_preserve_units_and_all_rows() {
        let mut server = mockito::Server::new_async().await;
        let now = unix_ms().expect("time");
        let clock = server
            .mock("GET", "/api/v5/public/time")
            .with_status(200)
            .with_body(format!(
                "{{\"code\":\"0\",\"data\":[{{\"ts\":\"{now}\"}}]}}"
            ))
            .create_async()
            .await;
        let public = server.mock("GET", "/api/v5/public/instruments")
            .match_query(mockito::Matcher::Exact("instType=SPOT".into()))
            .match_header("x-simulated-trading", "1")
            .with_status(200)
            .with_body(r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SPOT","state":"live","baseCcy":"BTC","quoteCcy":"USDT","tickSz":"0.1","lotSz":"0.0001","minSz":"0.001","groupId":"1"},{"instId":"ETH-USDT","instType":"SPOT","state":"post_only","baseCcy":"ETH","quoteCcy":"USDT","tickSz":"0.01","lotSz":"0.001","minSz":"0.01","groupId":"1"}]}"#)
            .create_async().await;
        let targeted = server.mock("GET", "/api/v5/public/instruments")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("instType".into(), "SPOT".into()),
                mockito::Matcher::UrlEncoded("instId".into(), "BTC-USDT".into()),
            ]))
            .with_status(200)
            .with_body(r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SPOT","state":"live","baseCcy":"BTC","quoteCcy":"USDT","tickSz":"0.1","lotSz":"0.0001","minSz":"0.001","groupId":"1"}]}"#)
            .create_async().await;
        let account = server.mock("GET", "/api/v5/account/instruments")
            .match_query(mockito::Matcher::UrlEncoded("instType".into(), "SPOT".into()))
            .match_header("ok-access-key", "test-key")
            .with_status(200)
            .with_body("{\"code\":\"0\",\"data\":[{\"instId\":\"BTC-USDT\",\"instType\":\"SPOT\"},{\"instId\":\"ETH-USDT\",\"instType\":\"SPOT\"}]}")
            .create_async().await;
        let fee = server.mock("GET", "/api/v5/account/trade-fee")
            .match_query(mockito::Matcher::AllOf(vec![mockito::Matcher::UrlEncoded("instType".into(), "SPOT".into()), mockito::Matcher::UrlEncoded("groupId".into(), "1".into())]))
            .match_header("ok-access-key", "test-key")
            .with_status(200)
            .with_body(format!("{{\"code\":\"0\",\"data\":[{{\"instType\":\"SPOT\",\"ts\":\"{now}\",\"feeGroup\":[{{\"groupId\":\"1\",\"maker\":\"-0.0008\",\"taker\":\"-0.001\"}}]}}]}}"))
            .create_async().await;
        let client = ReadOnlyClient::build(&server.url(), creds()).expect("client");
        client.synchronize_clock().await.expect("clock");
        let specs = client
            .public_instruments(InstrumentType::Spot)
            .await
            .expect("public catalog");
        assert_eq!(specs.value.len(), 2);
        assert_eq!(
            specs.value[0].lot_size,
            Decimal::parse("0.0001").expect("lot")
        );
        assert_eq!(specs.value[1].state, InstrumentState::PostOnly);
        assert!(specs.source_time_ms.get() <= specs.received_time_ms.get());
        let single = client
            .public_instrument(
                InstrumentType::Spot,
                &OkxInstrumentId::new("BTC-USDT").expect("ID"),
            )
            .await
            .expect("allowlisted instrument");
        assert_eq!(
            single.to_instrument_spec().expect("spec").base.as_str(),
            "BTC"
        );
        let visible = client
            .account_instruments(
                AccountId::new("demo-a").expect("account ID"),
                InstrumentType::Spot,
            )
            .await
            .expect("account catalog");
        assert_eq!(visible.value.inst_ids.len(), 2);
        let rates = client
            .fee_group(InstrumentType::Spot, "1")
            .await
            .expect("fee group");
        assert_eq!(rates.value.maker, Decimal::parse("-0.0008").expect("maker"));
        assert_eq!(rates.value.updated_time_ms.get(), now);
        let specs = specs
            .to_instrument_specs()
            .expect("versioned specifications");
        let mapping = instrument::ProductMapping {
            product_id: model::identity::ProductId::new("spot-btc").expect("product ID"),
            okx_inst_id: OkxInstrumentId::new("BTC-USDT").expect("exchange ID"),
            kind: ProductKind::Spot,
            base: Currency::new("BTC").expect("base"),
            quote: Currency::new("USDT").expect("quote"),
            settlement: None,
        };
        let directory =
            instrument::Directory::new(vec![mapping.clone()], specs).expect("directory");
        let config = Observation {
            value: AccountConfiguration {
                account_level: "2".into(),
                position_mode: PositionMode::NetMode,
            },
            source_time_ms: visible.source_time_ms,
            received_time_ms: visible.received_time_ms,
        };
        let permit = instrument::AccountPermit {
            account_id: visible.value.account_id.clone(),
            allowed_modes: vec![PositionMode::NetMode],
            product_ids: vec![mapping.product_id.clone()],
        };
        let eligibility = visible.to_eligibility(&config);
        let fee_schedule = rates
            .to_fee_schedule(mapping.okx_inst_id)
            .expect("fee fact");
        let checked_at = ReceivedTimeMs::new(unix_ms().expect("time")).expect("time");
        assert!(
            directory
                .assess(
                    &mapping.product_id,
                    &permit,
                    &eligibility,
                    &fee_schedule,
                    checked_at,
                    10_000
                )
                .is_ok()
        );
        clock.assert_async().await;
        public.assert_async().await;
        targeted.assert_async().await;
        account.assert_async().await;
        fee.assert_async().await;
    }

    #[tokio::test]
    async fn mock_rejects_oversize_and_bad_business_payload() {
        let mut server = mockito::Server::new_async().await;
        let oversized = server
            .mock("GET", "/api/v5/public/time")
            .with_status(200)
            .with_body("x".repeat(MAX_BODY_BYTES + 1))
            .create_async()
            .await;
        let client = ReadOnlyClient::build(&server.url(), creds()).expect("client");
        assert_eq!(
            client.synchronize_clock().await,
            Err(Error::ResponseTooLarge)
        );
        oversized.assert_async().await;
        let malformed = server
            .mock("GET", "/api/v5/public/time")
            .with_status(200)
            .with_body("{\"code\":\"0\",\"data\":[{\"ts\":123}]}")
            .create_async()
            .await;
        assert_eq!(
            client
                .get_rows::<TimeRow>("/api/v5/public/time", false)
                .await
                .err(),
            Some(Error::InvalidResponse)
        );
        malformed.assert_async().await;
    }

    #[test]
    fn swap_requires_linear_usdt_contract_units() {
        let raw = r#"{"instId":"BTC-USDT-SWAP","instType":"SWAP","state":"live","tickSz":"0.1","lotSz":"1","minSz":"1","ctType":"linear","ctVal":"0.01","ctValCcy":"BTC","settleCcy":"USDT","groupId":"2"}"#;
        let row: InstrumentRow = serde_json::from_str(raw).expect("swap row");
        let swap = row.parse(InstrumentType::Swap).expect("linear swap");
        assert_eq!(swap.base.as_str(), "BTC");
        assert_eq!(swap.quote.as_str(), "USDT");
        assert_eq!(
            swap.contract_value,
            Some(Decimal::parse("0.01").expect("value"))
        );
        for bad in [
            raw.replace("\"linear\"", "\"inverse\""),
            raw.replace("\"USDT\"", "\"USD\""),
            raw.replace("\"ctVal\":\"0.01\"", "\"ctVal\":\"0\""),
        ] {
            let row: InstrumentRow = serde_json::from_str(&bad).expect("syntactically valid row");
            assert_eq!(row.parse(InstrumentType::Swap), Err(Error::InvalidResponse));
        }
    }

    #[test]
    fn private_reads_require_recent_server_clock() {
        let client = ReadOnlyClient::build("http://127.0.0.1:1/", creds()).expect("test client");
        assert_eq!(client.timestamp(), Err(Error::ClockUnavailable));
        let mut clock = client.clock.lock().expect("clock lock");
        *clock = Some(ClockSync {
            at: Instant::now() - CLOCK_MAX_AGE - Duration::from_secs(1),
            server_ms: 1,
            local_ms: 1,
        });
        drop(clock);
        assert_eq!(client.timestamp(), Err(Error::ClockUnavailable));
    }
}
