# HANDOVER — fix/select-highlight-bug

Branch: `fix/select-highlight-bug`
HEAD: `c5044ee fix(preview): restore breadcrumb scrolling after compact dismissal (#1173)`
Working tree at time of handover: **clean** (no code changes committed or staged by the investigating agent).

---

## 1. Original task / problem

When moving the selection with the keyboard, the selection **border (outline)** visibly moves first and the **selection highlight/fill (background)** updates slightly afterward, producing a brief frame where border and highlight are out of sync.

Requirements from the requester:

- Keyboard navigation must change selection border and highlight **together**, as one visual state.
- No intermediate frame may show the border on one item while the highlight is on another.
- Must hold for **single selection and multi-selection**.
- **Preserve** existing border geometry, color, thickness, and selection behavior.
- Do **NOT** add delays, timers, or animation.
- Do **NOT** hide the issue with CSS *masking* (the requester wants the underlying state/update ordering fixed).
- Make the **smallest possible change**; keep unrelated behavior untouched.
- Run relevant tests, `cargo fmt`, clippy, `git diff --check`.
- Do **NOT** commit, push, or rebase.

Note: the requester's "do not hide with CSS" constraint was interpreted as "no masking hacks (fake outlines, shadow overlays, opacity tricks)"; removing a *root-caused* transition mismatch is a state-ordering-class fix, but the next agent should confirm this interpretation is acceptable before touching `style.css` (see §8).

---

## 2. Investigation findings and confirmed root cause

### Event/update ordering is CORRECT — it is not a Rust ordering bug

The full keyboard→state→paint path was traced end-to-end and all state updates for border and highlight happen in the **same synchronous pass**:

- Keyboard dispatch (`src/ui/window/keyboard/items.rs` → `selection_navigation`, `native_selection`, `directory_navigation`) calls into `src/app/browser.rs` (`move_selection`, `extend_selection`, `page_along`, `extend_visual_selection`).
- Those mutate `src/app/navigation.rs` state (`focus_only`, `extend_selection`, `extend_visual_selection`) — `column.selected`, `selected_locations`, `selection_anchor` are all updated **atomically in one borrow**.
- Then a single `BrowserEvent::FocusChanged` (single-select) or `BrowserEvent::SelectionSetChanged` (multi-select) is emitted.
- The UI observer (`src/ui/browser.rs:592` → `ViewState::handle` in `src/ui/browser/events.rs`) applies both visuals from the **same event**:
  - `FocusChanged` handler (events.rs:558) computes `selected_positions(depth)` and calls `set_column_selections(&column, &positions)` — the selection model (background) is set from the same event that moves the cursor/focus (border).
  - `SelectionSetChanged` handler (events.rs:523) calls `set_column_selections(column, &filtered_positions)` — again one synchronous update.
- `set_column_selections` (`src/ui/browser/columns.rs:324`) sets the `gtk::MultiSelection` bitset synchronously (via `apply_selection_plan`, `src/ui/browser/collection.rs:600`), with `syncing_selection` guard set/cleared around it.
- The keyboard cursor outline is a GTK **focus** outline (`row:focus-within` / `.keyboard-cursor` class + `:selected` background). Both are driven by GTK widget state (focus + selection bitset), not by separately-timed app code. `refresh_destination_style` (`src/ui/browser.rs:2046`) and the bind-cursor logic in `src/ui/browser/columns/rows.rs:746-757` also run in the same pass.
- Single-pane/list/icons modes: `src/ui/browser_modes/events.rs` `handle_selection_event` (line 286) calls `update_selection`/`update_panes` → `set_selections` (`src/ui/browser_modes.rs:4115`) synchronously; `focus_visible_pane` (`browser_modes.rs:1372`) moves the focus cursor in the same event handling. The only deferrals there are idle/tick callbacks for **scroll-into-view and grab-focus when widgets are not yet allocated** (`focus_collection_item_when_allocated`, `scroll_collection_when_allocated`, `restore_column_cursor`) — these defer *focus placement*, not the selection bitset, and only matter when rows are not yet allocated. They are not the reported lag during ordinary arrow-key movement.

**Confirmed root cause: a CSS transition-duration mismatch in `src/style.css`.**

- `.file-row, .list-row` (style.css ~line 4033) declare:
  `transition: background 220ms ease-out, color 180ms ease-out, outline-color 220ms ease-out, transform 200ms cubic-bezier(...), opacity 200ms ease-out;`
- The row's own selection highlight (`.file-list row:selected` at line 4353 sets `background: alpha(@theme_accent, 0.22)`) animates on the row element with a **220 ms** `background` transition, while
- the keyboard cursor border (`.keyboard-navigation .file-list:focus-within .file-row.keyboard-cursor ...` at line 4369 sets `outline: 1px solid @theme_accent`) animates `outline-color` on the **inner `.file-row` element**, also 220 ms — but the *fill* that visually reads as the "highlight" for the cursor row can also come from `row:hover/:selected` backgrounds on the **parent `row` element** (line 4021 block: `transition: background 160ms ease-out, color 160ms ease-out` on `.file-list row`).

So the border (outline-color, 220 ms) and the fill (background, 160 ms on the row / 220 ms on `.file-row`) are animated by **different elements with different transition properties and durations**. When the cursor row changes, the old row's outline fades out over 220 ms while the new row's fill snaps in at 160 ms (or vice versa), producing the visible "border moved first / highlight lagged" artifact. This matches the report exactly: both visuals are set in the same frame, but their *transitions* finish at different times.

The transition on `.file-row, .list-row` was introduced in commit `87d67bd` (feat preview drag, #935) and earlier in `9749534`; the 160 ms row background transition dates to `af7bbef` (#554). The border color/width itself was last changed by `5f6616c` (fix(highlight): fix selection border styling, #1165) — that commit only changed color/width (`@theme_text` 2px → `@theme_accent` 1px), **not** transitions, and is not the cause.

Icons mode has no transition on its elements (`.icons-card`, `.file-icons > child` have no `transition` declaration; verified lines 4867–4960), so icons mode is not affected by this specific mismatch, but the fix should keep it that way (see §9).

---

## 3. Files inspected and relevant code paths

Read and traced (no changes made to any):

- `src/ui/window/keyboard/items.rs` — `item_navigation`, `single_pane_navigation`, `native_selection`, `selection_navigation`, `directory_navigation` (keyboard entry points).
- `src/ui/window/keyboard/commands.rs` — `text_input` (calls `view.keyboard_navigation()` for nav keys), `clipboard_command`.
- `src/app/browser.rs` — `move_selection` (2254), `extend_selection` (2275), `extend_visual_selection`, `page_along`, `focus_parent/focus_child`, `set_selection` (1420, emits `SelectionSynced`), `select_all` (1449), `commit_selection`.
- `src/app/navigation.rs` — `select` (781), `commit_selection` (795), `set_selection` (797), `extend_selection` (843), `extend_visual_selection` (918), `selected_positions` (983), `move_selection` (1032), `page_along` (1110), `focus_only` (1286). All state mutations are single-borrow/atomic.
- `src/ui/browser.rs` — observer registration (line 592), `keyboard_navigation` (1279), `refresh_destination_style` (2046, applies `keyboard-cursor` css class per bound row), `select_all`, `sync_mode_selection` (1948), `claim_keyboard_navigation` (244).
- `src/ui/browser/events.rs` — `ViewState::handle` dispatch; `SelectionSetChanged` handler (line 523), `FocusChanged` handler (line 558), `event_refreshes_active_path` (1061) → `refresh_active_path_rows` (runs after both handlers in the same pass).
- `src/ui/browser/columns.rs` — `set_column_selections` (324), `apply_selection_plan` usage, `restore_column_cursor` (287, tick-callback focus restore), selection `connect_selection_changed` (790, pointer path → `browser.set_selection` + `refresh_destination_style`), column build `append_column` (633).
- `src/ui/browser/columns/rows.rs` — factory `setup`/`bind` (keyboard-cursor class applied at bind, lines 746–757), click/marquee handlers.
- `src/ui/browser/collection.rs` — `scroll_collection_when_allocated` (44), `focus_collection_item_when_allocated` (49), `apply_collection_scroll` (197), `apply_selection_plan` (600).
- `src/ui/browser_modes.rs` — `focus_visible_pane` (1372), `connect_selection` (4036), `set_selections` (4115), `focus_collection_cursor_when_bound` (4172), `collection_keeps_cursor` (4202), `focus_bound_cursor` (4214), `sync_browser_selection` (4813), list/icons view creation (2335, 2976).
- `src/ui/browser_modes/events.rs` — `handle_with_deferred_empty` (13), `handle_selection_event` (286), `update_selection` (317).
- `src/style.css` — key blocks:
  - 4021 `.file-list row, .file-list-mode row` — `transition: background 160ms ease-out, color 160ms ease-out;`
  - 4033 `.file-row, .list-row` — `outline: 1px solid transparent; transition: background 220ms, color 180ms, outline-color 220ms, transform 200ms, opacity 200ms`
  - 4353 `.file-list row:selected, .file-list-mode row:selected` — highlight fill `alpha(@theme_accent, 0.22)`
  - 4369 keyboard cursor border selector — `outline: 1px solid @theme_accent; outline-offset: -1px;`
  - 4921 `.file-icons > child:focus-visible .icons-card-icon-frame` — icons-mode cursor border (no transitions on these elements)
  - 5114 `.keyboard-navigation .file-list-mode row:focus-within` — list-mode cursor border
- `git show 5f6616c` — prior border-styling fix (color/width only; did not touch transitions).

---

## 4. Exact changes already made

**Second attempt (current working tree, `src/style.css` only):**

The first attempt (removing only `outline-color 220ms ease-out` from the `.file-row, .list-row` transition, §8 option (a)) did **not** fix the artifact — with the border snapping instantly, the highlight *fill* was still animated (`background 160ms` on `row`, `220ms` on `.file-row`), so fill and border remained out of sync.

Second attempt removes the **`background` transition** from both row blocks so the selection fill switches in the same frame as the border:

1. `.file-list row, .file-list-mode row` (~line 4021): `transition: background 160ms ease-out, color 160ms ease-out;` → `transition: color 160ms ease-out;`
2. `.file-row, .list-row` (~line 4033): removed `background 220ms ease-out` (the `outline-color 220ms ease-out` removal from attempt 1 is retained): `transition: color 180ms ease-out, transform 200ms cubic-bezier(0.2, 0.9, 0.3, 1.2), opacity 200ms ease-out;`

Rationale: border and fill are now both non-animated state flips applied from the same synchronous event pass (§2), so they cannot desync. Hover/entry/other visuals (color, transform, opacity) keep their existing animations. Icons mode was never animated and is untouched.

---

## 5. Current git diff / status

- `git status --short` → `M src/style.css`, untracked `HANDOVER.md`.
- `git diff` → exactly the two `transition` lines in §4; nothing else.
- Branch `fix/select-highlight-bug` at `c5044ee`; no commits made.
- **NOT committed or pushed** (per instructions).

---

## 6. Tests / checks already run and results (second attempt)

- `./scripts/test-headless.py ui::browser::tests::focus` → **12 passed, 0 failed, 2 ignored** (ignored ones require a mapped GTK window; selection/focus-cursor behavior covered, all green).
- `git diff --check` → clean.
- `cargo fmt --check` → clean (native substitute).
- `cargo clippy --all-targets --all-features` → no warnings.
- **Not run:** `./scripts/quality.sh fmt` / `clippy` — they pull the pinned E2E base and the Docker socket is inaccessible in this environment (`permission denied ... /var/run/docker.sock`). Native equivalents were run instead; owner should rerun the pinned checks before pushing if CI parity is desired.
- Visual verification (arrow keys, Shift+arrows, hjkl; list/columns/icons) still requires a manual run by the owner — no automated paint-timing test is permitted per repo rules.

---

## 7. Unresolved

1. ~~**No fix has been applied yet.**~~ A fix **is applied** in the working tree (§4, second attempt: background + outline-color transitions removed from rows). Needs visual confirmation by the owner; if the artifact still reproduces, the remaining suspects are outside `src/style.css` (re-verify with GTK inspector which element paints the lag).
2. Interpretation of the "do NOT hide with CSS" constraint for a transition-alignment fix needs owner sign-off (a transition change is arguably "state-ordering" in the render pipeline, but it *is* an edit to `style.css`).
3. Whether multi-select (`SelectionSetChanged` path) shows the same artifact in practice — the transition mismatch affects the `row`/`.file-row` elements identically, so it should, but it has not been visually verified.
4. No Rust-side regression test exists (or is known) that could catch a paint-timing artifact; per repo rules, layout/pixel-timing tests are forbidden, so verification is manual/GUI-based.

---

## 8. Exact next steps for the next agent

1. **Confirm the interpretation** of "no CSS masking" with the owner: the proposed fix removes a *transition mismatch*, it does not add a mask/hack. If the owner insists on a Rust-side change instead, see the alternative in §10.
2. **Apply the minimal fix in `src/style.css`:** make the border and the highlight animate identically (or not at all) on the row that carries the keyboard cursor. Two candidate minimal edits (pick one, smallest first):
   - **(a)** In the `.file-row, .list-row` block (~line 4033), drop `outline-color 220ms ease-out` from the `transition` list so the outline snaps instantly while the fill transitions — or
   - **(b)** align durations so both read as one state, e.g. change `outline-color 220ms ease-out` → `outline-color 160ms ease-out` **and** `background 220ms ease-out` → `background 160ms ease-out` in that same block, matching the 160 ms row transitions at line 4021.
   Option (a) is the smallest change and cannot desync (border never animates). Preserve `outline: 1px solid @theme_accent; outline-offset: -1px` geometry/color/thickness exactly as set by commit `5f6616c`.
3. Do **not** touch the selectors themselves, the `:selected` background colors, `keyboard-cursor` class logic, or any Rust file for this fix.
4. **Verify visually** (manual): build/run, use arrow keys (single selection), Shift+arrows (multi selection), and hjkl vim navigation in List, Columns, and Icons views; confirm no frame shows border/highlight on different rows. Icons mode should be unaffected both before and after.
5. **Run checks** (repo policy: targeted tests only, fmt/clippy at pre-push checkpoint):
   - `git diff --check`
   - `./scripts/quality.sh fmt`
   - `./scripts/quality.sh clippy`
   - Targeted tests via `./scripts/test-headless.py ui::browser::tests::focus` (covers keyboard-cursor class behavior) — the filter must match at least one test; record the count. Do **not** rerun the full Rust suite for this change.
   - Relevant E2E if desired: `./scripts/e2e.sh tests/e2e/scenarios/test_keyboard_navigation.py` (pinned-container runner; do not rebuild the base).
6. **Do NOT commit, push, or rebase** unless the owner explicitly asks. Record scope rationale and results in the handoff/PR body if a PR is eventually opened (issue-first workflow per AGENTS.md).
7. Update the PR with before/after screen captures if the fix is user-visible (per AGENTS.md visual-evidence rules; keep captures out of the source tree).

---

## 9. Important constraints — what must NOT change

- Border geometry/color/thickness: `outline: 1px solid @theme_accent; outline-offset: -1px` (as of `5f6616c`) must be preserved exactly for all three cursor selectors (list-columns 4369, icons 4921, list-mode 5114).
- Selection behavior/state code in `src/app/*`, `src/ui/browser*`, `src/ui/window/keyboard/*` — event ordering is already correct; do not restructure it.
- No delays, timers, tick callbacks, or animation added.
- No masking hacks (fake borders, box-shadow outlines, opacity overlays).
- Repo rules that bind: never write layout/pixel-geometry tests; no test implementations inline with production code; keep Lucide/theming conventions; icons/theme colors via `@theme_*` only (the existing CSS already complies — keep it that way); one conversation/one PR; scoped tests only; `./scripts/quality.sh fmt` + `clippy` before any push.
- Do not modify the pinned media-runtime patches, E2E base images, or `target/quality-container`/`target/e2e-container` caches.

---

## 10. Hypotheses investigated but NOT confirmed / alternatives

1. **Rust state-update ordering bug (border CSS class applied before selection bitset):** traced all keyboard paths — the selection bitset and cursor class/focus are updated in the same synchronous event pass. **Not the cause.** If the owner rejects a CSS fix entirely, the Rust-side alternative would be forcing a single combined repaint (e.g., one `queue_draw` after both updates), but nothing in the current code splits the updates across frames, so this is unlikely to be the actual mechanism — do not attempt it before re-verifying the visual artifact.
2. **Deferred focus/scroll callbacks (`focus_collection_cursor_when_bound`, `restore_column_cursor`, `scroll_collection_when_allocated` tick callbacks):** these defer *focus placement* only until widgets are allocated (max 8 frames), not the selection fill. They matter on cold layout/mode switches, not on ordinary arrow-key movement where rows are already allocated. **Not the cause of the reported lag**, and should not be touched.
3. **`SelectionSynced`-event gating:** `SelectionSynced` is intentionally ignored by the view handlers (events.rs:51) to avoid feedback loops with the pointer path; keyboard paths emit `FocusChanged`/`SelectionSetChanged` instead. Working as designed; **not related**.
4. **GTK native `row:focus-within` outline vs app-driven `keyboard-cursor` class double-application:** both selectors resolve to the same outline on the same element; the desync comes from transition timing, not selector duplication.
5. **Icons-mode artifact:** `.icons-card`/`.file-icons > child` have no transitions at all (verified), so icons mode should not exhibit the lag; if the owner reports it there too, re-investigate — the current root cause does not explain icons mode.
