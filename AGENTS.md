# Hyperion Agent Constitution

This document defines the default operating rules for any coding agent working in Hyperion. It is a project handbook, not a roster of roles. Use it to keep changes aligned with the existing architecture, UI system, verification standards, and documentation practices of this cross-platform Matrix client.

## Stack And Repo Map

Hyperion is a cross-platform Matrix client built with Tauri, React, TypeScript, Pnpm (Node.js), Rust, and Cargo.

Primary repo areas:

- `src/`: React and Vite UI code.
- `src/components/`: shared React UI, contexts, hooks, theme primitives, and Storybook stories.
- `src/components/hooks/useColorScheme.ts`: system color-scheme detection hook.
- `src/components/themes/`: current palette and design-token sources.
- `src/components/storybook/`: isolated stories for shared UI components and design tokens.
- `src-tauri/`: Rust and Tauri backend code.
- `src-tauri/gen/`: tracked platform scaffolding and generated platform project files.
- `dist/` and `node_modules/`: ignored outputs and dependencies. Do not hand-edit these.

## JavaScript Package Manager

- pnpm is the only JavaScript package manager for this repo.
- Use `pnpm`, `pnpm run`, `pnpm exec`, and `pnpm tauri ...`.
- Do not use `bun`, `bunx`, `npm`, `npx`, `yarn`, or `pnpx` unless the user explicitly asks or a specific external tool requires it.
- Translate stale examples automatically: `bun run build` -> `pnpm run build`, `bun lint` -> `pnpm lint`, `bunx eslint ...` -> `pnpm exec eslint ...`, `bun tauri ...` -> `pnpm tauri ...`.

## Architecture Principles

### Default architectural rule: Rust owns Matrix logic.

Apply that rule as follows:

- Keep Matrix, network, session, account, persistence, and business logic in Rust behind Tauri commands whenever practical.
- For Rust backend work that touches `matrix-sdk`, double-check the implementation against the official Matrix Rust SDK documentation: `https://matrix-org.github.io/matrix-rust-sdk/matrix_sdk/`.
- Use React and TypeScript primarily for UI composition, presentation state, adaptive layouts, and invoking backend commands.
- Avoid duplicating Matrix or account logic in the frontend unless there is a clear UI-only reason.
- Prefer extending existing Rust command surfaces over re-implementing backend behavior on the frontend.
- Avoid duplicating the same logical state across Rust services, Tauri events, React context, component state, and cached DTOs. Prefer one canonical owner for each piece of state, then derive or project from that owner.
- Treat timelines, virtualization, media rendering, search, and sync-driven UI updates as performance-sensitive surfaces. Avoid unnecessary rerenders, cloning, serialization, and repeated derived computations in these areas.

When choosing where new behavior belongs:

- Put platform access, secure storage, session handling, protocol behavior, and durable logic in Rust.
- Put screen flow, form state, rendering logic, local interaction state, and view-specific composition in React.

### Shared UI host-integration boundary:

- Keep `src/components/` completely free of Tauri dependencies. Do not import `@tauri-apps/*` or other native host APIs from shared components, contexts, hooks, themes, or Storybook stories.
- Shared UI must use framework-neutral local ports, props, or contexts for host capabilities such as theme persistence and window controls.
- Put the concrete Tauri implementations for those ports under `src/utils/tauri/`. Wire them into the application only at the composition root or another explicit host boundary.
- Do not move Tauri-specific behavior back into `src/components/` to simplify a single component; extend the relevant local adapter or port instead.

### Shell backend structure note:

- `ShellManager` is the Tauri-managed shell facade and shared lifecycle root. Keep it small: it should own long-lived shell services, coordinate account/sync teardown, and expose command-facing methods, but it should not accumulate feature-specific Matrix workflows or caches directly.
- Put shell feature behavior in focused service modules under `src-tauri/src/shell/service/`. Current examples include room workflows in `room/commands.rs`, timeline command orchestration in `room/timeline/commands.rs`, timeline/discovery service state in `runtime.rs`, cache state in `caching/cache_state.rs`, discovery workflows in `discovery/commands.rs`, and search workflows in `global_search.rs` plus `search/`.
- Sync service layering is split by responsibility under `src-tauri/src/shell/service/sync/`. `ShellSyncCoordinator` is the high-level shell sync/timeline coordinator, while `ShellSyncManager` in `src-tauri/src/shell/service/sync/matrix_sdk.rs` is the low-level Matrix SDK sync-service adapter. Keep these layers separate: the coordinator may depend on the SDK adapter, `ShellTimelineService`, and shell event emitters, but do not merge adapter behavior into the coordinator or route shell callers directly to SDK sync internals.
- `src-tauri/src/shell/service/sync_coordinator.rs` is a compatibility re-export/shim for the split sync coordinator module. Do not add new coordinator behavior there.

### Matrix sync and timeline invariants:

- `matrix_sdk_ui::Timeline` together with the encrypted Matrix SDK EventCache is the sole authority for visible timeline events, local echoes, send states, edits, reactions, redactions, and visible pagination.
- Do not add a Hyperion-owned persisted timeline-event cache, local-/remote-echo reconciliation, body/time-window matching, a parallel send-state machine, or custom event deduplication. The former `timeline-view.sqlite3` cache is legacy cleanup only and must not be revived or migrated.
- SDK EventCache and `/messages` reads may support bounded background warmup, unread recovery, or search indexing, but must not become a second visible-timeline authority.
- Preserve the current Tauri timeline DTO and event payloads; project them from the SDK timeline rather than persisting a separate projection.
- Sync-state classification must match typed Matrix SDK/Ruma errors. Never infer `offline` or `unsupported` from formatted error text. Keep the raw SDK error only as diagnostic detail.
- Per active account there is exactly one focused room. Only an explicit UI room open or switch may change it; search, warmup, pagination, event-context loading, sending, and background refreshes must not.
- Prioritize the focused room with `RoomListService::subscribe_to_rooms`. On `SyncService` start or restart, subscribe the current focus again.
- Typing is scoped to the focused room: hold one listener for it, clear the prior room on focus change, discard stale updates, and reject outbound typing for a non-focused room.
- On account switch, logout, and service stop, clear focus and abort its typing task.

- When adding a new shell capability, first decide whether it fits an existing service. Extend that service and have `ShellManager` delegate to it instead of adding broad new fields or large workflow methods directly to `ShellManager`.
- Tauri command and facade methods are responsible for resolving the active account. For Matrix-facing work that requires a logged-in account, call `AccountManager::require_active_account(&app)` at the command/facade boundary and pass the resulting `ActiveAccount` into lower shell, settings, search, discovery, timeline, and Matrix helper methods. For commands that intentionally support a signed-out state, use `AccountManager::optional_active_account(&app)` at the same boundary and handle `None` there.
- Treat `ActiveAccount` as the proof that account loading and active-account selection already happened. Do not repeat `ensure_loaded(app)` plus `active_account_client_loaded()` inside Matrix workflow methods, and do not pass `AccountManager` into helpers just to rediscover the same active account.
- Do not thread `tauri::AppHandle` through Matrix-related functions by default. Keep `AppHandle` at the Tauri/runtime boundary and pass it deeper only when the callee directly needs Tauri capabilities such as app paths, secure storage, plugin APIs, dialogs/filesystem access, event emission, mobile WebView/activity context, sync startup, or background task event bridges.
- If the capability needs its own durable state, background tasks, caches, or cohesive lifecycle, add a small service struct near the relevant module and store that struct on `ShellManager`. Give the service narrow methods that take concrete inputs such as `ActiveAccount`, `AccountClientSnapshot`, request types, sync/cache/search dependencies, or existing runtime handles.
- Keep Tauri command names and frontend IPC shapes at the facade boundary. Internally, prefer passing the smallest dependency needed by the service. Do not pass `ShellManager`, `AccountManager`, or `AppHandle` into helpers just to reach unrelated state; pass the specific service, active account, runtime handle, or value instead.

## Code Style

### Language standards:

- Use TypeScript for frontend work.
- Do not introduce plain JavaScript unless the repo already requires it in a specific place.

### UI, layout, and design-token rules:

- UI work must support adjustable layouts that behave well on both mobile and desktop. Do not build desktop-only or phone-only layouts unless the task explicitly targets a platform-specific surface.
- Reuse the existing color and color-scheme system before introducing new visual values.
- Keep `src/components/themes/colorpalette.ts` focused on color palettes. Do not add typography, spacing, layout, shape, shadow, motion, or component tokens to this file.
- Put non-color design primitives in dedicated files under `src/components/themes/`, such as `typography.ts`, `spacing.ts`, `sizing.ts`, `shape.ts`, `elevation.ts`, `motion.ts`, and `layout.ts`.
- Use long, self-documenting token names. Prefer names like `radiusSmall`, `gapExtraLarge`, and `fontWeightBold` over abbreviations like `radiusSm`, `gapXl`, or `fwBold`.
- Treat primitives as low-level design values only: color palette, typography, spacing, sizing, shape, elevation/shadows, motion, and layout. Do not define React components in primitive files.
- `ThemeContext` may expose palette and primitive values as CSS variables, but it must remain primitive-only and must not define or render UI components.
- Shared UI patterns belong in `src/components/ui/`. Screens should compose shared components instead of duplicating button, input, card, feedback, badge, heading, or page-shell styling.
- Every new reusable UI component implemented as a `.tsx` file under `src/components/ui/` must have a corresponding Storybook story under `src/components/storybook/`. The story must cover the default state and the component's meaningful interactive, disabled, error, responsive, or overflow states.
- Keep Storybook stories separate from production component files under `src/components/storybook/`; do not place `*.stories.tsx` files inside `src/components/ui/` or `src/components/themes/`.
- Storybook stories must remain Tauri-free and use deterministic browser-safe adapters or mocks.
- Prefer the shared toast notification system for transient error, warning, info, and success feedback instead of rendering inline feedback boxes in screens. Keep inline feedback only when the content must remain persistently visible for the workflow, such as generated recovery keys, confirmation instructions, or durable status details.
- If a UI decision would produce a substantial or broadly useful component, prefer adding a reusable version under `src/components/ui/` instead of keeping a large one-off implementation inside a screen.
- Prefer the shared custom scrollbar pattern for scrollable UI. Use `src/components/ui/ScrollArea.tsx` and the existing `ui-scroll-area` / OverlayScrollbars styling instead of relying on native full scrollbars.
- Custom scrollbars must match the existing Hyperion scrollbar appearance: show only the scroll thumb, show it only when content actually overflows, do not render up/down or left/right arrow buttons, keep the same thickness as the other custom scrollbars, and preserve the existing primary-color glow/focus styling when the scrollbar is selected or active.
- Shared components must consume theme primitives through CSS variables. They must not define new one-off font sizes, radii, shadows, spacing scales, or layout constants unless the value is trivial and local.
- Default component corner rounding is small. Use the shared 8dp-equivalent radius token unless a component has a documented reason to differ.
- New screen work must first check existing primitives and `src/components/ui/` components before adding new local CSS.
- Settings screen work under `src/screens/Settings/` should keep each detail section in its own `.tsx` file, such as `Appearance.tsx`, `Account.tsx`, or `Sessions.tsx`, instead of accumulating all settings content in `SettingsView.tsx`.
- If a new reusable design value is needed, add it as a named primitive token before using it in components.
- Use `src/components/hooks/useColorScheme.ts` for system color-scheme integration.
- Prefer separate `.css` files for styling instead of embedding styles directly inside `.tsx` files, unless the existing local pattern or a task-specific constraint clearly requires otherwise.
- Avoid redundant CSS declarations that only restate browser or layout defaults, such as `width: 100%`, `height: auto`, `flex: 0 1 auto`, `opacity: 1`, or zero-valued properties, unless the declaration is required to override inherited styles, satisfy a layout constraint, or document a deliberate non-default behavior.
- Exception: viewport-bounded flex/grid scrolling is allowed to use declarations like `min-height: 0`, `max-height: 100%`, or similar defaults when they are necessary to make nested panes scroll correctly. In those cases, keep the declaration and add a short comment explaining why the bound is required.
- Do not leave empty CSS rulesets behind after refactors or cleanup passes. If a selector no longer needs declarations, remove the ruleset entirely instead of keeping an empty block as a placeholder.
- Prefer `lucide-react` for icons when an icon package is needed.
- When fonts, shapes, spacing, font sizes, or other design tokens become centralized, reuse those declarations instead of adding local one-off values.
- Back navigation should be visually quiet by default. Use the shared `BackButton` component where possible. It should be an icon-only `ghost` button with an accessible label, a compact square hit area, and top-left placement within the current screen or logical panel. It should not include visible text, should not use bordered secondary-button styling, and should not consume a full dedicated row. Prefer colocating it with the related heading or overlaying it in the panel's top-left corner when the heading is centered. Back buttons must stay compact on mobile even though other shared icon-only action buttons may become full-width in some mobile contexts.

### Avoid magic numbers:

- In React and TypeScript, prefer named constants, shared tokens, or clearly scoped configuration values over unexplained inline literals.
- In Rust, prefer named constants or well-scoped configuration values when a number carries business meaning, platform meaning, layout meaning, or reuse value.
- If a literal is truly trivial and local, keep it local. If it encodes intent, name it.

### Implementation defaults:

- Prefer explicit names over clever shortcuts.
- Avoid bare underscore placeholders when a meaningful name is possible. Do not use imports like `use x as _` when `use x` or a named import makes the dependency clear.
- Avoid parameter lists like `_, _, value`; use descriptive names so the call shape remains readable.
- In Rust, prefer underscore-prefixed names such as `_app: &AppHandle<R>` for intentionally unused parameters instead of adding `#[allow(unused_variables)]`. Use `#[allow(unused_variables)]` only when a narrow, local underscore-prefixed binding cannot express the intent.
- In Rust type positions, avoid `_` inference placeholders when spelling out the type improves readability. Prefer explicit forms like `collect::<HashSet<&str>>()`, `collect::<HashMap<&str, &RoomTimelineItem>>()`, and `collect::<Vec<RoomTimelineItem>>()` over `HashSet<_>`, `HashMap<_, _>`, or `Vec<_>`.
- Treat Clippy warnings as defects to fix. Do not add `#[allow(...)]`, crate-level suppressions, or lint-specific bypasses for Clippy warnings unless the warning is truly unavoidable and the code cannot be made clearer or more correct another way.
- If a Clippy suppression is absolutely necessary, keep it as narrow as possible and add a short comment explaining why fixing the warning would make the code worse or impossible.
- Preserve Cargo workspace wildcard members such as `plugins/*` in `Cargo.toml`. Do not expand wildcard workspace members into explicit package paths unless the user specifically asks for that change.
- Reuse existing project patterns before introducing new abstractions.
- Keep public interfaces and command shapes consistent with the surrounding code.
- Prefer removing obsolete code over wrapping it in compatibility abstractions. Prefer fewer concepts over highly generic systems unless reuse is already proven.
- Do not introduce abstraction layers preemptively. Add adapters, managers, providers, registries, factories, coordinators, or similar layers only when they remove real complexity or match an established local pattern.
- Treat file length as a maintainability signal, not a strict quota. When a file becomes difficult to scan, mixes multiple responsibilities, or requires frequent jumping between unrelated sections, extract cohesive parts into nearby modules or files.
- Prefer extraction by responsibility, not by line count. Good split points include domain types, command handlers, service logic, UI subcomponents, hooks, constants, and focused CSS files.
- Do not split a file just to reduce line count if the result creates awkward indirection or separates code that must be read together.
- When a file is growing because of a new feature, consider creating the supporting module or component before the file becomes hard to review.
- When modeling Rust data with structs, place domain, command, response, and shared data shapes in a nearby `types.rs` module. Very small private helper structs may stay local only when they are not reused and do not describe public behavior.
- For Rust modules with reusable error types, repeated error mapping, domain-specific error constructors, or meaningful conversions, extract that error handling into a nearby `errors.rs` module.
- Do not create `errors.rs` only for one-off `map_err` messages or tiny private helpers. Keep trivial local error handling near the code it explains.
- Prefer named error helpers over repeated string formatting when the same failure mode appears in multiple places.
- For Rust errors that never reach the frontend, use the shared tracing wrapper in `src-tauri/src/utils/tracing.rs` rather than calling the `tracing` crate directly. Errors deliberately returned to the frontend may use normal error handling at that boundary.
- Keep `if` / `else` branching shallow without eliminating it entirely. Prefer not to exceed 3 nested conditional layers.
- In Rust, prefer simple guard clauses, early returns, small `if` checks, helper functions, or named intermediate booleans when nested branches would get deeper.
- Use Rust `match` when it makes enum, state, or small shape-based branching clearer. Do not replace readable `if` checks with large pattern matches across many variables; long match arms over 6 or more inputs are usually harder to scan than several simple checks.
- In TypeScript and React, use guard clauses, early returns, extracted helper functions, derived booleans, or small render helpers instead of deeply nested conditionals. Do not collapse many independent UI conditions into one large conditional expression when several simple checks are easier to read.
- If deeper nesting is genuinely clearer for localized parser, state-machine, or platform-specific logic, add a short comment explaining why the structure is intentional.
- Prefer readable named functions, branch-first flow, or intermediate variables over dense inline conditionals and anonymous function expressions when the logic is non-trivial.
- In TypeScript and React, avoid nested ternaries, multi-branch inline conditional assignments, complex inline arrow functions, and IIFEs inside JSX. Extract the logic into named helpers, derived variables, or small render functions.
- Prefer splitting long async method chains into intermediate variables for clarity.
- Use descriptive intermediate variables when a chain exceeds 3 methods or crosses line width.

Commenting rules:

- Never delete existing comments unless the user specifically instructs you to remove them.
- When adding comments, prefer explaining why the code exists or why a decision was made instead of restating what the code already does.
- Comment difficult, non-obvious, or easily misread code paths so that a developer with moderate experience can understand the intent without reverse-engineering it.
- When introducing a named constant in Rust or TypeScript, add a small comment that explains what it is for and why it should be used. This is especially expected for file-level constants.
- Do not comment everything. Keep comments selective and focused on the parts that benefit from extra context.

## Editing Boundaries And Change Scope

Versioned files are generally editable, including tracked platform scaffolding under `src-tauri/gen/`, when the task genuinely requires it.

Editing defaults:

- Prefer changing the true source of behavior instead of patching downstream generated artifacts when both are viable.
- Do not hand-edit ignored outputs such as `dist/` or dependencies inside `node_modules/`.
- Preserve existing license headers, copyright headers, and surrounding file conventions where present.
- When editing large files, make changes in manageable batches instead of sending thousands of lines at once. This avoids PowerShell command-length limits and keeps edits easier to review.
- Keep changes scoped to the request.
- Do not simultaneously rewrite multiple architectural layers in one pass unless explicitly instructed. Prefer stabilizing one layer before modifying dependent layers.
- Avoid combining DTO rewrites, Tauri IPC changes, Rust service rewrites, virtualization changes, rendering rewrites, interaction rewrites, and CSS redesigns into one large change.
- Avoid unrelated refactors unless they are required to complete the task safely.
- Do not revert or overwrite unrelated user changes.

## Dependencies And Implementation Preferences

Use a conservative dependency policy.

Dependency defaults:

- Prefer the existing stack, platform APIs, standard library, and current dependencies first.
- Add new Rust crates or npm packages only when there is a clear need and ask for permission beforehand.
- Do not add dependencies speculatively.
- Avoid adding dependencies that substantially overlap with tools already present in the repo.
- When adding a dependency, keep the choice narrow and explain why existing options were insufficient.
- Explain why existing platform APIs, current dependencies, or a small local utility are insufficient before introducing a new package or crate.
- Add npm dependencies with `pnpm add <package> --filter <workspace-package> --save-catalog`.
- Add Cargo dependencies by first writing the dependency in the root `Cargo.toml` with `default-features = false`, then adding it to the appropriate workspace member with `cargo add <package> -p <workspace-package>`.

## Verification Expectations

Cross-layer verification is the default after meaningful code changes. Even when a change looks isolated, prefer checking both frontend and backend impact where reasonable.

Required verification commands:

- `just test`
- `just check`
- `just fmt`
- `cargo clippy --fix ...` when Clippy suggests a concrete automatic fix command instead of manual fixing.
- relevant `pnpm tauri ...` commands when platform or integration behavior is affected
- `just storybook` when Storybook configuration, stories, or shared UI presentation changes.

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
- Treat `src/components/storybook/static/` as generated Storybook output; keep it ignored by `src/components/.gitignore` and do not hand-edit or commit it.

Testing expectations:

- Prefer adding or updating tests alongside meaningful behavior changes instead of relying only on manual verification.
- Write tests first, verify that they fail for the missing behavior, and only then write the accompanying implementation.
- Rust unit tests should stay close to the code they exercise, usually in the same `.rs` file under an inline `#[cfg(test)] mod tests { ... }` module. A separate test-only source file under `src-tauri/src/` is acceptable only when keeping the tests inline would make the production module unreasonably large or hard to scan.
- Rust integration tests should live under `src-tauri/tests/`. Add integration coverage when a change affects user-facing command behavior, persistence flows, cross-module service behavior, or any path where multiple backend modules must work together correctly.
- Do not turn private implementation details public only to make an integration test possible. If the behavior is still internal and narrow, keep it as a unit test. If the behavior is observable through a public API, command-facing facade, or stable service boundary, prefer an integration test in addition to focused unit tests.
- Organize `src-tauri/tests/` by behavior area, such as `search.rs`, `account.rs`, or `settings.rs`, when integration coverage grows. Keep shared integration-test fixtures small and explicit.

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

### tauri

Use for Tauri shell work: IPC, commands and events, native APIs, plugins, permissions, security, mobile support, debugging, configuration, packaging, signing, and CI/CD. For this repository, use the repo-local skill at `.agents/skills/tauri/` and prefer pnpm command variants.

### rust-skills

Use for all meaningful Rust work, including Matrix and Tauri backend code. Apply the relevant rules for ownership, errors, async/concurrency, types, serde, performance, testing, observability, and Clippy rather than loading every rule indiscriminately.

### vercel-composition-patterns

Use when designing or refactoring reusable React component APIs, especially components with many boolean props, compound components, context/providers, lifted state, explicit variants, or render-prop alternatives.

### vercel-react-best-practices

Use for React performance work: async waterfalls, bundle size, data fetching, rerenders, effects, large-list rendering, event listeners, and expensive client-side computation. Prefer the client/rendering guidance for this Vite/Tauri app; Next.js server-specific rules apply only if such a surface is introduced.

### vercel-react-view-transitions

Use only for intentional React View Transition API behavior: route or list-to-detail transitions, shared elements, enter/exit animations, list reordering, Suspense reveals, or directional navigation. Follow its audit and CSS-reference workflow, respect reduced motion, and verify local React/browser support before adding dependencies or APIs.

### web-design-guidelines

Use for explicit UI, UX, accessibility, or interface-quality reviews. Fetch the latest guidelines before reviewing and report findings in the requested `file:line` format; it is not required for every UI implementation change.
