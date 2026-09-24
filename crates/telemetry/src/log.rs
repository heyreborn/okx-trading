//! Allowlisted structured events for cross-node correlation. No free-form
//! message or credential field is accepted by this API; output is JSON stdout.

use std::fmt;

/// Process name included in every event and metric.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Service {
    Ingest,
    Archive,
    Observe,
    Trader,
    Notifier,
    Admin,
}

impl Service {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ingest => "ingest",
            Self::Archive => "archive",
            Self::Observe => "observe",
            Self::Trader => "trader",
            Self::Notifier => "notifier",
            Self::Admin => "admin",
        }
    }
}

/// Stable reason code. Callers must not place raw errors or responses in logs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Reason {
    Ready,
    DependencyUnavailable,
    FactsStale,
    QueueFull,
    ConfigMismatch,
    Shutdown,
}

impl Reason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::DependencyUnavailable => "dependency_unavailable",
            Self::FactsStale => "facts_stale",
            Self::QueueFull => "queue_full",
            Self::ConfigMismatch => "config_mismatch",
            Self::Shutdown => "shutdown",
        }
    }
}

/// Stable event name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    Started,
    HealthChanged,
    InputRejected,
    Stopped,
}

impl Event {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::HealthChanged => "health_changed",
            Self::InputRejected => "input_rejected",
            Self::Stopped => "stopped",
        }
    }
}

/// Safe correlation fields. IDs are bounded and contain no control characters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Context {
    service: Service,
    instance_id: String,
    config_version: String,
    event_id: Option<String>,
}

/// Invalid correlation field name; supplied values are never echoed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InstanceId,
    ConfigVersion,
    EventId,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid correlation field {self:?}")
    }
}

impl std::error::Error for Error {}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b':' | b'.'))
}

impl Context {
    /// Creates bounded log fields. No I/O occurs; bad IDs return field-only errors.
    pub fn new(service: Service, instance_id: &str, config_version: &str) -> Result<Self, Error> {
        if !valid_id(instance_id) {
            return Err(Error::InstanceId);
        }
        if !valid_id(config_version) {
            return Err(Error::ConfigVersion);
        }
        Ok(Self {
            service,
            instance_id: instance_id.to_owned(),
            config_version: config_version.to_owned(),
            event_id: None,
        })
    }

    /// Adds a bounded cross-process event ID; does not read or write external state.
    pub fn with_event_id(mut self, event_id: &str) -> Result<Self, Error> {
        if !valid_id(event_id) {
            return Err(Error::EventId);
        }
        self.event_id = Some(event_id.to_owned());
        Ok(self)
    }
}

/// Installs the JSON stdout subscriber once per process. Initialization fails
/// if another global subscriber is already installed; it never sends network data.
pub fn init_json() -> Result<(), tracing::subscriber::SetGlobalDefaultError> {
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_ansi(false)
        .with_writer(std::io::stdout)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
}

/// Emits one allowlisted JSON event. Logging is best-effort and is never an
/// audit commit or approval signal; callers retain their own authoritative state.
pub fn emit(context: &Context, event: Event, reason: Reason) {
    tracing::info!(
        service = context.service.as_str(),
        instance_id = context.instance_id.as_str(),
        environment = "demo",
        config_version = context.config_version.as_str(),
        event_name = event.as_str(),
        reason_code = reason.as_str(),
        event_id = context.event_id.as_deref().unwrap_or("")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .expect("capture lock")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn json_fields_are_stable_and_bounded() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::clone(&bytes);
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_ansi(false)
            .with_writer(move || Capture(Arc::clone(&writer)))
            .finish();
        let context = Context::new(Service::Ingest, "node-1", "v1")
            .expect("valid context")
            .with_event_id("event:42")
            .expect("valid event id");
        tracing::subscriber::with_default(subscriber, || {
            emit(&context, Event::InputRejected, Reason::FactsStale);
        });
        let captured = bytes.lock().expect("capture lock");
        let json: serde_json::Value = serde_json::from_slice(&captured).expect("JSON log");
        assert_eq!(json["fields"]["service"], "ingest");
        assert_eq!(json["fields"]["reason_code"], "facts_stale");
        assert_eq!(json["fields"]["event_id"], "event:42");
        assert!(json.get("level").is_some());
        assert!(json.get("timestamp").is_some());
        assert!(Context::new(Service::Trader, "node\nsecret", "v1").is_err());
    }

    struct FailedSink;

    impl Write for FailedSink {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("unavailable"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("unavailable"))
        }
    }

    #[test]
    fn failed_log_sink_does_not_gate_business_state() {
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_writer(|| FailedSink)
            .finish();
        let context = Context::new(Service::Trader, "node-1", "v1").expect("valid context");
        let mut persisted_fact = false;
        tracing::subscriber::with_default(subscriber, || {
            emit(
                &context,
                Event::HealthChanged,
                Reason::DependencyUnavailable,
            );
            persisted_fact = true;
        });
        assert!(persisted_fact);
    }
}
