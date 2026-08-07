/*
 * Copyright (c) 2026 VinVel
 *
 * SPDX-License-Identifier: AGPL-3.0-only
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, version 3 only.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 *
 * Project home: hyperion.velcore.net
 */

#[cfg(debug_assertions)]
use std::collections::HashMap;
#[cfg(not(debug_assertions))]
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
#[cfg(debug_assertions)]
use tracing::{Event, Metadata, field::Visit};
#[cfg(not(debug_assertions))]
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::{
    EnvFilter, filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt,
};

/// Reverse-DNS identity used by every native logging backend.
#[cfg(any(
    target_os = "android",
    target_os = "ios",
    target_os = "linux",
    target_os = "macos",
    target_os = "windows"
))]
const NATIVE_LOG_IDENTIFIER: &str = "net.velcore.hyperion";

/// Debug builds expose complete Hyperion diagnostics and useful Matrix SDK detail by default.
#[cfg(debug_assertions)]
const DEBUG_DEFAULT_DIRECTIVES: &str =
    "hyperion=trace,hyperion_lib=trace,matrix_sdk=debug,tauri_plugin_tracing=debug";

/// Release builds only expose explicitly redacted Hyperion application targets.
#[cfg(not(debug_assertions))]
const RELEASE_DEFAULT_DIRECTIVES: &str = "hyperion=info,hyperion_lib=info,hyperion::frontend=info";

/// ETW keywords are bitmasks; zero has special semantics and must not be used.
#[cfg(target_os = "windows")]
const ETW_DEFAULT_KEYWORD: u64 = 1;

/// Keeps logging infrastructure conceptually tied to the application lifetime.
///
/// Native layers themselves are owned by the installed global subscriber. The
/// guard remains in `run` so future backend resources can be retained without
/// changing the initialization contract.
#[must_use]
pub struct TracingGuard {
    _private: (),
}

/// Adds one error event at a Tauri command boundary without changing its IPC result.
pub trait CommandResultExt<T> {
    fn report_command_failure(
        self,
        command: &'static str,
        component: &'static str,
    ) -> Result<T, String>;
}

impl<T> CommandResultExt<T> for Result<T, String> {
    fn report_command_failure(
        self,
        command: &'static str,
        component: &'static str,
    ) -> Result<T, String> {
        #[cfg(debug_assertions)]
        if let Err(error) = &self {
            tracing::error!(
                target: "hyperion",
                event_name = "command.failed",
                component,
                command,
                operation = command,
                error_code = "command.failed",
                error_category = "command",
                error_source = %error,
                "Tauri command `{command}` failed: {error}"
            );
        }

        #[cfg(not(debug_assertions))]
        if self.is_err() {
            tracing::error!(
                target: "hyperion",
                event_name = "command.failed",
                component,
                command,
                operation = command,
                error_code = "command.failed",
                error_category = "command",
                "Tauri command `{command}` failed"
            );
        }

        self
    }
}

/// Reports the final result of an asynchronous Tauri command at its existing boundary.
pub async fn report_command_future<T>(
    command: &'static str,
    component: &'static str,
    future: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    future.await.report_command_failure(command, component)
}

/// Reports the final result of a synchronous Tauri command at its existing boundary.
pub fn report_command_result<T>(
    command: &'static str,
    component: &'static str,
    result: Result<T, String>,
) -> Result<T, String> {
    result.report_command_failure(command, component)
}

/// Formats debug-only diagnostic fields and suppresses unchanged state snapshots.
#[cfg(debug_assertions)]
pub fn changed_diagnostic_fields(
    component: &'static str,
    event_name: &'static str,
    fields: &[(&str, &str)],
) -> Option<String> {
    static LAST_DIAGNOSTIC_STATES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

    let rendered_fields = fields
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<String>>()
        .join(" ");
    let identity_fields = fields
        .iter()
        .filter(|(name, _value)| {
            matches!(
                *name,
                "account_id" | "account_key" | "kind" | "list_kind" | "owner" | "room_id"
            )
        })
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<String>>()
        .join(" ");
    let diagnostic_key = format!("{component}:{event_name}:{identity_fields}");
    let states = LAST_DIAGNOSTIC_STATES.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut states) = states.lock() else {
        return Some(rendered_fields);
    };

    if states
        .get(&diagnostic_key)
        .is_some_and(|previous_fields| previous_fields == &rendered_fields)
    {
        return None;
    }

    states.insert(diagnostic_key, rendered_fields.clone());
    Some(rendered_fields)
}

/// Release diagnostics never inspect or format potentially identifying fields.
#[cfg(not(debug_assertions))]
pub fn changed_diagnostic_fields(
    component: &'static str,
    event_name: &'static str,
    _fields: &[(&str, &str)],
) -> Option<String> {
    static EMITTED_RELEASE_DIAGNOSTICS: OnceLock<Mutex<HashSet<(&'static str, &'static str)>>> =
        OnceLock::new();

    let diagnostics = EMITTED_RELEASE_DIAGNOSTICS.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut diagnostics) = diagnostics.lock() else {
        return Some(String::new());
    };
    diagnostics
        .insert((component, event_name))
        .then(String::new)
}

/// Records a final background-operation failure using the shared release policy.
pub fn report_background_error(
    component: &'static str,
    operation: &'static str,
    error_code: &'static str,
    error_category: &'static str,
    error: &(impl std::fmt::Display + ?Sized),
) {
    #[cfg(debug_assertions)]
    tracing::error!(
        target: "hyperion",
        event_name = "background.failed",
        component,
        operation,
        error_code,
        error_category,
        error_source = %error,
        "Background operation `{operation}` failed: {error}"
    );

    #[cfg(not(debug_assertions))]
    {
        let _ = error;
        tracing::error!(
            target: "hyperion",
            event_name = "background.failed",
            component,
            operation,
            error_code,
            error_category,
            "Background operation `{operation}` failed"
        );
    }
}

/// Records a recoverable item or optional follow-up failure.
pub fn report_recoverable_error(
    component: &'static str,
    operation: &'static str,
    error_code: &'static str,
    error_category: &'static str,
    error: &(impl std::fmt::Display + ?Sized),
) {
    #[cfg(debug_assertions)]
    tracing::warn!(
        target: "hyperion",
        event_name = "background.recovered",
        component,
        operation,
        error_code,
        error_category,
        error_source = %error,
        "Recoverable operation `{operation}` failed: {error}"
    );

    #[cfg(not(debug_assertions))]
    {
        let _ = error;
        tracing::warn!(
            target: "hyperion",
            event_name = "background.recovered",
            component,
            operation,
            error_code,
            error_category,
            "Recoverable operation `{operation}` failed"
        );
    }
}

/// Installs the native subscriber for the current platform.
///
/// Initialization failures intentionally disable logging instead of installing
/// a stderr fallback or preventing the application from starting.
pub fn initialize() -> Option<TracingGuard> {
    initialize_native().then_some(TracingGuard { _private: () })
}

fn environment_filter() -> EnvFilter {
    #[cfg(debug_assertions)]
    let (default_level, defaults) = (LevelFilter::WARN, DEBUG_DEFAULT_DIRECTIVES);
    #[cfg(not(debug_assertions))]
    let (default_level, defaults) = (LevelFilter::OFF, RELEASE_DEFAULT_DIRECTIVES);

    let directives = match std::env::var("RUST_LOG") {
        Ok(overrides) if !overrides.trim().is_empty() => format!("{defaults},{overrides}"),
        _ => defaults.to_owned(),
    };

    EnvFilter::builder()
        .with_default_directive(default_level.into())
        .parse_lossy(directives)
}

#[cfg(any(not(debug_assertions), test))]
fn release_target_is_allowed(metadata: &tracing::Metadata<'_>) -> bool {
    let target = metadata.target();
    target == "hyperion"
        || target.starts_with("hyperion::")
        || target == "hyperion_lib"
        || target.starts_with("hyperion_lib::")
}

/// Keeps Matrix SDK diagnostics while discarding its high-volume elapsed-time chatter.
#[cfg(debug_assertions)]
#[derive(Debug, Default)]
struct MatrixSdkTimerFilter;

#[cfg(debug_assertions)]
impl<S> tracing_subscriber::layer::Filter<S> for MatrixSdkTimerFilter {
    fn enabled(
        &self,
        _metadata: &Metadata<'_>,
        _context: &tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        true
    }

    fn event_enabled(
        &self,
        event: &Event<'_>,
        _context: &tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        !is_matrix_sdk_timer(event)
    }
}

#[cfg(debug_assertions)]
fn is_matrix_sdk_timer(event: &Event<'_>) -> bool {
    if !event.metadata().target().starts_with("matrix_sdk") {
        return false;
    }

    let mut visitor = EventMessageVisitor::default();
    event.record(&mut visitor);
    visitor
        .message
        .is_some_and(|message| message.starts_with("Timer _") && message.contains(" finished in "))
}

#[cfg(debug_assertions)]
#[derive(Debug, Default)]
struct EventMessageVisitor {
    message: Option<String>,
}

#[cfg(debug_assertions)]
impl Visit for EventMessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_owned());
        }
    }
}

#[cfg(debug_assertions)]
fn install_native_layer<L>(layer: L) -> bool
where
    L: tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
{
    tracing_subscriber::registry()
        .with(layer.with_filter(MatrixSdkTimerFilter))
        .with(environment_filter())
        .try_init()
        .is_ok()
}

#[cfg(not(debug_assertions))]
fn install_native_layer<L>(layer: L) -> bool
where
    L: tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
{
    tracing_subscriber::registry()
        .with(layer)
        .with(environment_filter())
        .with(filter_fn(release_target_is_allowed))
        .try_init()
        .is_ok()
}

#[cfg(target_os = "linux")]
fn initialize_native() -> bool {
    let Ok(layer) = tracing_journald::layer() else {
        return false;
    };

    install_native_layer(
        layer
            .with_syslog_identifier(NATIVE_LOG_IDENTIFIER.to_owned())
            .with_field_prefix(None),
    )
}

#[cfg(target_os = "windows")]
fn initialize_native() -> bool {
    let layer = tracing_etw::LayerBuilder::new(NATIVE_LOG_IDENTIFIER)
        .with_default_keyword(ETW_DEFAULT_KEYWORD)
        .build();
    let Ok(layer) = layer else {
        return false;
    };

    install_native_layer(layer)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn initialize_native() -> bool {
    install_native_layer(tracing_oslog::OsLogger::new(
        NATIVE_LOG_IDENTIFIER,
        "application",
    ))
}

#[cfg(target_os = "android")]
fn initialize_native() -> bool {
    use tracing_logcat::{LogcatMakeWriter, LogcatTag};

    let tag = LogcatTag::Fixed(NATIVE_LOG_IDENTIFIER.to_owned());
    let Ok(writer) = LogcatMakeWriter::new(tag) else {
        return false;
    };
    let layer = tracing_subscriber::fmt::layer()
        .compact()
        .without_time()
        .with_ansi(false)
        .with_writer(writer);

    install_native_layer(layer)
}

#[cfg(not(any(
    target_os = "android",
    target_os = "ios",
    target_os = "linux",
    target_os = "macos",
    target_os = "windows"
)))]
fn initialize_native() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{CommandResultExt, changed_diagnostic_fields, release_target_is_allowed};

    #[cfg(debug_assertions)]
    use super::{DEBUG_DEFAULT_DIRECTIVES, MatrixSdkTimerFilter};
    #[cfg(debug_assertions)]
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[cfg(debug_assertions)]
    use tracing_subscriber::{Layer, layer::SubscriberExt};

    #[test]
    fn release_target_policy_only_accepts_hyperion_targets() {
        let allowed = tracing::metadata! {
            name: "allowed",
            target: "hyperion::frontend",
            level: tracing::Level::INFO,
            fields: &[],
            callsite: &TEST_CALLSITE,
            kind: tracing::metadata::Kind::EVENT,
        };
        let denied = tracing::metadata! {
            name: "denied",
            target: "matrix_sdk",
            level: tracing::Level::INFO,
            fields: &[],
            callsite: &TEST_CALLSITE,
            kind: tracing::metadata::Kind::EVENT,
        };

        assert!(release_target_is_allowed(&allowed));
        assert!(!release_target_is_allowed(&denied));
    }

    #[test]
    fn command_failure_reporting_preserves_the_ipc_error_string() {
        let result = Err::<(), String>(String::from("Existing user-facing error"))
            .report_command_failure("test_command", "test");

        assert_eq!(result, Err(String::from("Existing user-facing error")));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_defaults_enable_hyperion_trace_and_matrix_sdk_debug() {
        assert!(DEBUG_DEFAULT_DIRECTIVES.contains("hyperion=trace"));
        assert!(DEBUG_DEFAULT_DIRECTIVES.contains("matrix_sdk=debug"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_native_layer_only_suppresses_matrix_sdk_timer_messages() {
        let event_count = Arc::new(AtomicUsize::new(0));
        let subscriber = tracing_subscriber::registry().with(
            EventCountingLayer {
                event_count: Arc::clone(&event_count),
            }
            .with_filter(MatrixSdkTimerFilter),
        );

        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(
                target: "matrix_sdk_sqlite::event_cache_store",
                "Timer _method_ finished in 1.2ms"
            );
            tracing::debug!(
                target: "matrix_sdk_sqlite::event_cache_store",
                item_count = 3,
                "Saved event cache changes"
            );
            tracing::debug!(
                target: "hyperion",
                "Timer _application_operation_ finished in 1.2ms"
            );
        });

        assert_eq!(event_count.load(Ordering::Relaxed), 2);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn unchanged_debug_diagnostic_state_is_suppressed() {
        let fields = [("account_key", "account"), ("outcome", "ready")];

        assert_eq!(
            changed_diagnostic_fields("test", "test.debug_state", &fields).as_deref(),
            Some("account_key=account outcome=ready")
        );
        assert_eq!(
            changed_diagnostic_fields("test", "test.debug_state", &fields),
            None
        );

        let changed_fields = [("account_key", "account"), ("outcome", "stopped")];
        assert!(changed_diagnostic_fields("test", "test.debug_state", &changed_fields).is_some());
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn release_diagnostic_deduplication_does_not_format_fields() {
        let fields = [("room_id", "!private:example.org")];

        assert_eq!(
            changed_diagnostic_fields("test", "test.release_state", &fields).as_deref(),
            Some("")
        );
        assert_eq!(
            changed_diagnostic_fields("test", "test.release_state", &fields),
            None
        );
    }

    struct TestCallsite;
    static TEST_CALLSITE: TestCallsite = TestCallsite;

    impl tracing::callsite::Callsite for TestCallsite {
        fn set_interest(&self, _interest: tracing::subscriber::Interest) {}

        fn metadata(&self) -> &tracing::Metadata<'_> {
            unreachable!("test metadata supplies this callsite directly")
        }
    }

    #[cfg(debug_assertions)]
    struct EventCountingLayer {
        event_count: Arc<AtomicUsize>,
    }

    #[cfg(debug_assertions)]
    impl<S> Layer<S> for EventCountingLayer
    where
        S: tracing::Subscriber,
    {
        fn on_event(
            &self,
            _event: &tracing::Event<'_>,
            _context: tracing_subscriber::layer::Context<'_, S>,
        ) {
            self.event_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}
