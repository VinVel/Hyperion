# Native diagnostics

Hyperion routes application runtime diagnostics through Rust's `tracing`
subscriber. Rust events and structured events emitted by the TypeScript helper
in `src/utils/tracing.ts` share the same native destination:

- Windows: ETW provider `net.velcore.hyperion`, keyword `1`
- Linux: systemd journal identifier `net.velcore.hyperion`
- macOS and iOS: OSLog subsystem `net.velcore.hyperion`, category `application`
- Android: Logcat tag `Hyperion`

There is deliberately no stdout, stderr, file, or WebView/DevTools logging
layer. If the native sink cannot be initialized, Hyperion continues to start
with tracing disabled.

## Levels and fields

Use `ERROR` for failed operations and managed background services, `WARN` for
recovery and skipped items, `INFO` for lifecycle changes, `DEBUG` for workflow
summaries, and `TRACE` for high-frequency timeline, sync, geometry, and
pagination details.

Stable field names include `account_id`, `room_id`, `matrix_event_id`,
`command`, `component`, `operation`, `outcome`, `error_code`,
`error_category`, `duration_ms`, `item_count`, `page_size`, `inserted_count`,
`duplicate_count`, and `removed_count`. Error codes describe the Hyperion
operation rather than its current implementation library.

Debug builds enable Hyperion backend and frontend events through `TRACE`,
`matrix_sdk` through `DEBUG`, plugin events through `DEBUG`, and other
third-party crates through `WARN`. No environment variable is required for
complete application diagnostics. `RUST_LOG` remains available to narrow or
override the defaults, for example:

```bash
RUST_LOG=hyperion=debug,matrix_sdk=info pnpm tauri dev
```

Timeline and sync `TRACE` diagnostics are therefore active in a normal debug
build. The previous query parameter, local-storage flag, and global timeline
dump object no longer exist. Disabled frontend levels are cached at startup
and do not issue IPC; disabled `TRACE` calls do not construct geometry
snapshots or item arrays. Repeated diagnostics with the same event identity
and unchanged fields are suppressed until their state changes.

Release builds accept `RUST_LOG` changes only for `hyperion`,
`hyperion_lib`, and `hyperion::frontend` targets. External Matrix SDK, HTTP,
SQL, and Tauri events cannot be enabled in a release build.

## Privacy

Debug builds may include Matrix identifiers, normal error chains, source
locations, JavaScript stacks, and diagnostic snapshots. Release builds retain
only approved event names, components, operations, error codes/categories,
outcomes, counters, durations, and other static status values. They remove
identifiers, URLs, message content, free-form messages, source errors,
callstacks, and arbitrary diagnostic payloads before constructing native
events.

Passwords, access or refresh tokens, recovery and store keys, secret-storage
keys, complete sessions, authorization headers, cookies, and cryptographic key
material must never be logged in any build.

## Reading native logs

Linux:

```bash
journalctl -t net.velcore.hyperion -f
```

Every event supplies a human-readable `MESSAGE` for this default view. To
inspect the same event's structured fields such as `EVENT_NAME`, `COMPONENT`,
`ROOM_ID`, or `ERROR_SOURCE`, use:

```bash
journalctl -t net.velcore.hyperion -f -o verbose
```

macOS:

```bash
log stream --predicate 'subsystem == "net.velcore.hyperion"'
```

iOS events can be filtered by the same subsystem in Console.app. On Windows,
capture the `net.velcore.hyperion` ETW provider with WPR, WPA, PerfView, or
TraceView. On Android:

```bash
adb logcat -s Hyperion
```

Platform release validation must confirm that identifiers, message content,
error chains, and stack traces are absent.
