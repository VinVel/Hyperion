# BetterTimeline-v4: Common Problems and Handoff Notes

This is a candid record of the issues encountered during the Robust Timeline
Semantics work. A listed “implemented” change means code was written and the
available automated checks passed; it does **not** mean the issue was manually
verified on every target platform.

The worktree is:

`/home/vinvel/Projekte/VSCodiumProjects/Hyperion/.trees/BetterTimeline-v4`

## Current priority: timeline pagination and viewport stability

These are the most important unresolved user-visible problems.

### The viewport moves when older messages are prepended

**Reported behavior:** When pagination completes, the messages visible in the
viewport change or are pushed away. This happens both when the user has
scrolled away from the oldest edge and when the user reaches the oldest event
and requests another page. The intended invariant is strict: the same visible
message content must remain in the same screen position after a successful
prepend.

**Why this is difficult:** Matrix timeline rows have variable heights. The
application uses `react-virtuoso`, which measures those rows asynchronously.
Virtuoso’s generic inverse-infinite-scroll API requires the item array and
`firstItemIndex` to change atomically. Competing timeline refreshes or manual
scroll corrections can break its measured anchor. There is also a current
upstream report involving variable-height prepend anchoring.

**Changes attempted:**

- Added and carried a `firstItemIndex` on `RoomTimeline`.
- Decremented it by the actual number of unique rows inserted on a backwards
  page.
- Replaced the inline pagination merge with
  `prependTimelinePage(...)` in `src/screens/app-shell/timeline/helpers.ts`.
- Removed a manual DOM/`scrollToIndex` viewport-restoration fallback. That
  fallback raced Virtuoso’s own anchor logic and used an unsuitable relative
  index after the absolute index base shifted.
- Removed `alignToBottom` from the `Virtuoso` instance because it is a separate
  short-list positioning policy.
- Deferred active-room SDK timeline refresh presentation while a pagination
  transaction is in flight, then merged the latest deferred SDK snapshot after
  the page update.

**Status:** Not manually verified after the latest refactor. The user had
reported that the prior implementation was still wrong. Do not claim this is
fixed until it is tested interactively with real, variable-height Matrix rows
in WebKitGTK.

**Relevant files:**

- `src/screens/app-shell/timeline/RoomTimelineView.tsx`
- `src/screens/app-shell/useAppShellState.ts`
- `src/screens/app-shell/timeline/helpers.ts`
- `src/screens/app-shell/timeline/TimelineScroller.tsx`

**Research already done:**

- Virtuoso documents `firstItemIndex` specifically for inverse scrolling and
  says it must decrease together with the prepended data.
- Virtuoso’s maintainer describes generic-list prepending as complicated and
  promotes the separate Message List package for this case. Do not add that
  package without explicit permission; it is a different product/licensing
  decision.
- Virtuoso troubleshooting warns that dynamic/variable-height content and
  margins can make reverse scrolling jump. Timeline styles were adjusted to
  avoid standard block margins, but this needs inspection in the running app.
- Upstream issue: <https://github.com/petyosi/react-virtuoso/issues/1405>
- Official API: <https://virtuoso.dev/react-virtuoso/api-reference/virtuoso/>
- Troubleshooting guide: <https://virtuoso.dev/react-virtuoso/troubleshooting/>

### Pagination can be initiated, but its visual feedback and trigger behavior need manual validation

**Requested behavior:** Pagination should happen only at the oldest edge when
the user tries to scroll further upward. It should have a small floating,
spinning loader without a background. A button is not wanted.

**Implemented changes:**

- Removed the “Load older messages” button/header.
- Added top-edge wheel intent handling in `TimelineScroller.tsx`.
- Added a floating `LoaderCircle` in `RoomTimelineView.tsx` while pagination is
  loading.
- Removed the loader background after feedback from the user.

**Status:** The loader’s visual appearance was accepted before the later
viewport work. Triggering and loader timing must still be manually retested
after the latest pagination refactor.

### Cursor continuation had runaway requests and local-storage quota crashes

**Reported behavior:** Continuing to scroll after a page request could advance
the cursor excessively, trigger 10–20 pages, and eventually crash with “The
quota has been exceeded.”

**Implemented changes:**

- Bounded automatic cursor continuation to three advances.
- Added bounded exponential backoff for retryable empty/duplicate pages.
- Made `localStorage` writes non-throwing.
- Limited durable cached timeline snapshots to 100 remote events; a truncated
  cache drops its pagination cursor because that cursor would no longer match
  the retained cache window.

**Status:** Unit-tested and should prevent the specific synchronous
`localStorage.setItem` crash path. It does not prove that every possible quota
source or every rate-limit path is safe.

### Rate-limit failures must be shown as an error toast, never as an interface crash

**Reported behavior:** Rate limiting / quota exhaustion could still lead to the
global “The interface crashed” screen. The user explicitly asked for an error
toast.

**Implemented changes:**

- Pagination error classification checks for known Matrix rate-limit signals
  and sends a specific feedback message: “Older messages are temporarily rate
  limited. Please wait and try again.”
- Cache writes no longer throw into React.

**Status:** Unit coverage exists for the string-level rate-limit classifier and
cache-write failure. This has not been tested against a real homeserver
`M_LIMIT_EXCEEDED` response. Also distinguish a Matrix rate limit from browser
storage quota exhaustion; they are different failures and need different
diagnostics.

### Fast WebKitGTK scrolling performance and end-of-list deceleration

**Reported behavior:**

- At fast scroll velocities, measured frame rate dropped from about 60 FPS to
  30–40 FPS.
- Near either end of the list, scrolling decelerated unexpectedly, even where
  the user expected constant wheel movement.
- A Logitech M705 can produce wheel input up to roughly 130,000 px/s; normal
  use was described as 5,000–20,000 px/s.

**Implemented changes:**

- Added a wheel-velocity cap of 15,000 px/s in `TimelineScroller.tsx`.
- Added `scrollSeekConfiguration`; entering the placeholder mode currently
  starts at 1,000 velocity units and exits at 50. These values are named
  constants specifically so they can be tuned.
- Added pseudo-text scroll-seek placeholders using horizontal lines rather
  than solid blocks.
- Reduced Virtuoso overscan from 320px to 120px in both directions.

**Status:** The user reported a performance improvement from the placeholders,
but did not consider the result finished. The end-of-list slowdown may be a
WebKitGTK/Virtuoso interaction and needs profiling with the current code. Do
not remove the velocity cap without testing the high-input-rate mouse case.
