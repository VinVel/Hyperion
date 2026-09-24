# Hyperion Agent Constitution

This document defines the default operating rules for any coding agent working in Hyperion. It is a project handbook, not a roster of roles. Use it to keep changes aligned with the existing architecture, UI system, verification standards, and documentation practices of this cross-platform Matrix client.

## Stack And Repo Map

Hyperion is a cross-platform Matrix client built with Qt (QML) & Rust.

Primary repo areas:

- `src/`: Rust Code
- `qml`: Qt QML Code
- `target`: ignored outputs and dependencies. Do not hand-edit.

## Architecture Principles

### Default architectural rule: Rust owns Matrix logic.

Apply that rule as follows:

- Keep everything that doesn't strictly belong into the Frontend (UI) in Rust accessible through ipc command in `cxxqt_objectrs` for the Qt Frontent whenever practical.
- For Rust backend work that touches `matrix-sdk`, double-check the implementation against the official Matrix Rust SDK documentation: `https://matrix-org.github.io/matrix-rust-sdk/matrix_sdk/`.
- Use Qt (QML) primarily for UI composition, presentation state, adaptive layouts, and invoking backend commands.
- Prefer extending existing Rust command surfaces over re-implementing backend behavior on the frontend.
- Avoid duplicating the same logical state across Rust or Qt. Prefer one canonical owner for each piece of state, then derive or project from that owner.
- Treat timelines, virtualization, media rendering, search, and sync-driven UI updates as performance-sensitive surfaces. Avoid unnecessary rerenders, cloning, serialization, and repeated derived computations in these areas.

When choosing where new behavior belongs:

- Put platform access, secure storage, session handling, protocol behavior, and durable logic in Rust.
- Put screen flow, form state, rendering logic, local interaction state, and view-specific composition in Qt.

### Shell backend structure note:

- `ShellManager` is the Tauri-managed shell facade and shared lifecycle root. Keep it small: it should own long-lived shell services, coordinate account/sync teardown, and expose command-facing methods, but it should not accumulate feature-specific Matrix workflows or caches directly.
- Put shell feature behavior in focused service modules under `src/shell/service/`. Current examples include room workflows in `room/commands.rs`, timeline command orchestration in `room/timeline/commands.rs`, timeline/discovery service state in `runtime.rs`, cache state in `caching/cache_state.rs`, discovery workflows in `discovery/commands.rs`, and search workflows in `global_search.rs` plus `search/`.
- Sync service layering is split by responsibility under `src/shell/service/sync/`. `ShellSyncCoordinator` is the high-level shell sync/timeline coordinator, while `ShellSyncManager` in `src/shell/service/sync/matrix_sdk.rs` is the low-level Matrix SDK sync-service adapter. Keep these layers separate: the coordinator may depend on the SDK adapter, `ShellTimelineService`, and shell event emitters, but do not merge adapter behavior into the coordinator or route shell callers directly to SDK sync internals.
- `src/shell/service/sync_coordinator.rs` is a compatibility re-export/shim for the split sync coordinator module. Do not add new coordinator behavior there.

### Matrix sync and timeline invariants:

- `matrix_sdk_ui::Timeline` together with the encrypted Matrix SDK EventCache is the sole authority for visible timeline events, local echoes, send states, edits, reactions, redactions, and visible pagination.
- Sync-state classification must match typed Matrix SDK/Ruma errors. Never infer `offline` or `unsupported` from formatted error text. Keep the raw SDK error only as diagnostic detail.

- When adding a new shell capability, first decide whether it fits an existing service. Extend that service and have `ShellManager` delegate to it instead of adding broad new fields or large workflow methods directly to `ShellManager`.
- If the capability needs its own durable state, background tasks, caches, or cohesive lifecycle, add a small service struct near the relevant module and store that struct on `ShellManager`. Give the service narrow methods that take concrete inputs such as `ActiveAccount`, `AccountClientSnapshot`, request types, sync/cache/search dependencies, or existing runtime handles.

## Code Style

### Language standards:

- Use Qt Quick (QML) for frontend work.
- When using Qt make sure that everything you do is Wayland compatible on Linux 
- Use Rust for backend work.
- For iOS native Code try to use Rust > Swift > C++
- For Android native Code try to use Rust (JNI) > Kotlin

### UI, layout, and design-token rules:

- UI work must support adjustable layouts that behave well on both mobile and desktop. Do not build desktop-only or phone-only layouts unless the task explicitly targets a platform-specific surface.
- Make use of the Qt Material3 Style on all OS Platforms, not just Android
- For Material.roundedScale always use Small scale

### Avoid magic numbers:

- Prefer named constants, shared tokens, or clearly scoped configuration values over unexplained inline literals.
- If a literal is truly trivial and local, keep it local. If it encodes intent, name it.

### Implementation defaults:

- Prefer explicit names over clever shortcuts.
- Avoid bare underscore placeholders when a meaningful name is possible. Do not use imports like `use x as _` when `use x` or a named import makes the dependency clear.
- Avoid parameter lists like `_, _, value`; use descriptive names so the call shape remains readable.
- In Rust, prefer underscore-prefixed names such as `_app: &AppHandle<R>` for intentionally unused parameters instead of adding `#[allow(unused_variables)]`. Use `#[allow(unused_variables)]` only when a narrow, local underscore-prefixed binding cannot express the intent.
- In Rust type positions, avoid `_` inference placeholders when spelling out the type improves readability. Prefer explicit forms like `collect::<HashSet<&str>>()`, `collect::<HashMap<&str, &RoomTimelineItem>>()`, and `collect::<Vec<RoomTimelineItem>>()` over `HashSet<_>`, `HashMap<_, _>`, or `Vec<_>`.
- Treat Clippy warnings as defects to fix. Do not add `#[allow(...)]`, crate-level suppressions, or lint-specific bypasses for Clippy warnings unless the warning is truly unavoidable and the code cannot be made clearer or more correct another way.
- If a Clippy suppression is absolutely necessary, keep it as narrow as possible and add a short comment explaining why fixing the warning would make the code worse or impossible.
- Reuse existing project patterns before introducing new abstractions.
- Keep public interfaces and command shapes consistent with the surrounding code.
- Prefer removing obsolete code over wrapping it in compatibility abstractions. Prefer fewer concepts over highly generic systems unless reuse is already proven.
- Do not introduce abstraction layers preemptively. Add adapters, managers, providers, registries, factories, coordinators, or similar layers only when they remove real complexity or match an established local pattern.
- Treat file length as a maintainability signal, not a strict quota. When a file becomes difficult to scan, mixes multiple responsibilities, or requires frequent jumping between unrelated sections, extract cohesive parts into nearby modules or files.
- Prefer extraction by responsibility, not by line count. Good split points include domain types, command handlers, service logic, UI subcomponents, hooks, constants, and focused QSS files.
- Do not split a file just to reduce line count if the result creates awkward indirection or separates code that must be read together.
- When a file is growing because of a new feature, consider creating the supporting module or component before the file becomes hard to review.
- For Rust errors that never reach the frontend, use the shared tracing wrapper in `src/utils/tracing.rs` rather than calling the `tracing` crate directly. Errors deliberately returned to the frontend may use different error handling at that boundary.
- Keep `if` / `else` branching shallow without eliminating it entirely. Prefer not to exceed 3 nested conditional layers.
- In Rust, prefer simple guard clauses, early returns, small `if` checks, helper functions, or named intermediate booleans when nested branches would get deeper.
- Use Rust `match` when it makes enum, state, or small shape-based branching clearer. Do not replace readable `if` checks with large pattern matches across many variables; long match arms over 6 or more inputs are usually harder to scan than several simple checks.
- Prefer readable named functions, branch-first flow, or intermediate variables over dense inline conditionals and anonymous function expressions when the logic is non-trivial.
- Prefer splitting long async method chains into intermediate variables for clarity.
- Use descriptive intermediate variables when a chain exceeds 3 methods or crosses line width.

Commenting rules:

- Never delete existing comments unless the user specifically instructs you to remove them.
- When adding comments, prefer explaining why the code exists or why a decision was made instead of restating what the code already does.
- Comment difficult, non-obvious, or easily misread code paths so that a developer with beginner experience can understand the intent without reverse-engineering it.
- When introducing a named constant in Rust or Qt, add a small comment that explains what it is for and why it should be used. This is especially expected for file-level constants.
- Do not comment everything. Keep comments selective and focused on the parts that benefit from extra context.

## Editing Boundaries And Change Scope

- Prefer changing the true source of behavior instead of patching downstream generated artifacts when both are viable.
- Do not hand-edit ignored outputs such as `target`.
- Preserve existing license headers, copyright headers, and surrounding file conventions where present.
- When editing large files, make changes in manageable batches instead of sending thousands of lines at once. This avoids PowerShell command-length limits and keeps edits easier to review.
- Keep changes scoped to the request.
- Do not simultaneously rewrite multiple architectural layers in one pass unless explicitly instructed. Prefer stabilizing one layer before modifying dependent layers.
- Avoid unrelated refactors unless they are required to complete the task safely.
- Do not revert or overwrite unrelated user changes.

## Dependencies And Implementation Preferences

Use a conservative dependency policy.

Dependency defaults:

- Prefer the existing stack, platform APIs, standard library, and current dependencies first.
- Add new Rust crates only when there is a clear need and ask for permission beforehand.
- Do not add dependencies speculatively.
- Avoid adding dependencies that substantially overlap with tools already present in the repo.
- When adding a dependency, keep the choice narrow and explain why existing options were insufficient.
- Explain why existing platform APIs, current dependencies, or a small local utility are insufficient before introducing a new package or crate.

## Verification Expectations

Cross-layer verification is the default after meaningful code changes. Even when a change looks isolated, prefer checking both frontend and backend impact where reasonable.

Required verification commands:

- `just test`
- `just check`
- `just fmt`
- `cargo clippy --fix ...` when Clippy suggests a concrete automatic fix command instead of manual fixing.

Verification rules:

- Run the relevant checks you can run in the current environment, prefer the `just *` commands over others.
- Prefer reporting both what passed and what was not run.
- When `cargo clippy` reports warnings, fix the underlying issue instead of suppressing it by default. Treat suppression as a last resort that must be justified in code.
- If Clippy suggests a concrete automatic fix command such as `cargo clippy --fix ...`, prefer running that command before manually patching the issue, then inspect the resulting diff for correctness.
- Before applying automatic fixes for experimental, noisy, or readability-sensitive lints, evaluate whether the lint should instead be allowed narrowly on the affected function or item.
- Allow Clippy warnings only on a function-by-function or item-by-item basis when the lint is experimental, contradicts the intended local style, or would make the code less clear if followed mechanically.
- When using a suppression, make sure to give the reason in the reason field, e.g.

```rust
#[allow(
    clippy::lint_name,
    reason = r#"REASON"#
)]
```

- If platform-specific verification is not practical, say so explicitly.
- If a change affects frontend-backend integration, do not report only one side.

Testing expectations:

- Add or update tests alongside meaningful behavior changes instead of relying only on manual verification.
- Important!: Write tests FIRST, verify that they fail for the missing behavior, and only then write the accompanying implementation.
- Rust unit tests should stay close to the code they exercise, usually in the same `.rs` file under an inline `#[cfg(test)] mod tests { ... }` module. A separate test-only source file under `src/` is acceptable only when keeping the tests inline would make the production module unreasonably large or hard to scan.
- Rust integration tests should live under `src/tests/`. Add integration coverage when a change affects user-facing command behavior, persistence flows, cross-module service behavior, or any path where multiple backend modules must work together correctly.
- Do not turn private implementation details public only to make an integration test possible. If the behavior is still internal and narrow, keep it as a unit test. If the behavior is observable through a public API, command-facing facade, or stable service boundary, prefer an integration test in addition to focused unit tests.
- Organize `src/tests/` by behavior area, such as `search.rs`, `account.rs`, or `settings.rs`, when integration coverage grows. Keep shared integration-test fixtures small and explicit.

## Documentation Placement

Prefer agent-facing and operational notes in `.agents/` when the information is mainly useful for local workflow, future agent context, or non-repo operational guidance.

Current local-doc behavior:

- Most of `.agents/` is currently ignored through `.gitignore`.
- That makes `.agents/` suitable for local operational documentation that should not be committed by default.

Tracked documentation rules:

- Update tracked docs such as `README.md` when setup steps, commands, platform requirements, or user-visible behavior materially change.
- Keep local agent notes in `.agents/` when they are not meant to become shared project documentation.

## Collaboration And Handoff

Before making decisions, ground them in the current repo state rather than assumptions.

Planning mode rules:

- In planning mode, assume as little as possible. Ask enough clarifying questions to lock down intended behavior, edge cases, constraints, and success criteria before presenting an implementation plan.
- Prefer asking over guessing when a decision would affect user-visible behavior, data flow, architecture, platform support, persistence, security, or testing expectations.
- If a detail can be discovered from the repository, inspect the repo first. Ask the user only for intent, preference, or missing context that cannot be derived from the code.

Working rules:

- Keep changes scoped and intentional.
- Make assumptions explicit when the repo does not fully answer a question.
- Call out important tradeoffs when there were multiple reasonable approaches.
- Surface incomplete verification, platform gaps, and follow-up risks clearly.
- Summarize notable architectural or implementation decisions after changes.
- Prefer named constants over inline literals in both TypeScript and Rust when values carry meaning or are likely to be reused.

This file applies equally to any agent working in Hyperion. It does not define specialist personas. It defines the shared quality bar and default behavior expected for all agent-driven work in this repository.

## Skill Guidance

### rust-skills

Use for all meaningful Rust work.

### qt-*

Use when involving yourself in any way with Qt. For research into qt make use of the qt mcp.