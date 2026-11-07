# Proposal: one bind system for global and navigation-mode keys

Status: approved 2026-11-07, with the decisions below.

## Problem

A key press can be resolved by two unrelated systems.

| | Global binds | Navigation-mode binds |
| --- | --- | --- |
| Config | `[binds]`, `-o b.key=...` | `[nav.binds]`, `--nav-bind`, profile defaults |
| Type | `BindMap<A>` keyed by `Trigger { kind, mode }` | `HashMap<String, Actions<NullActionExt>>` keyed by key name |
| Where | Event loop (`event.rs::get_bind`) | Render loop rewrites the batch afterwards (`render/keys.rs`, ~600 of its 721 lines) |
| Modes | Yes (`Trigger.mode`) | No; "results focused" acts as an implicit mode |
| Sequences | No | Only the hard-coded `g` prefix (`gg`, `gt`, `gb`) |
| Custom actions | Yes | No (`NullActionExt`); converted by `action_from_null` |
| In bind snapshot and help | Yes | Partly |

The navigation path also hard-codes keys that users can't change:

- the fullscreen and diagram preview keys (`preview_key_action`, 18 entries);
- the preview fallbacks for J/K/u/d/s/+/-/0/]/[, each written as "unless the user bound this key";
- editing actions translated back into key names (`DeleteChar` → `backspace`).

Effects:

- Bugs live in the gap. The dropped `@alias` bug came from the two paths resolving aliases differently.
- Every new feature that touches keys has to be done twice.
- The render loop simulates focus (`sim_focus`) across a batch of keys to guess what the event loop should have done.
- `keys.rs` is the most special-cased file in the repo: mutation testing found 56 of its 137 viable mutants untested, and 7 new tests were needed to pin its behaviour.

## Proposal

Make navigation mode a **bind mode** of the one `BindMap`, resolved in the event loop.

1. **Focus moves into `InputState`.** It is already shared by the event loop and the render loop. The event loop updates it itself when it emits `ToggleFocus`, `FocusFilter` or `FocusNav`, which replaces `sim_focus`.
2. **`[nav.binds]` becomes mode-scoped binds.** At config load they are translated into `Trigger { Key(..), mode: "nav" }` entries, following the partial-merge model. The config syntax doesn't change. The event loop looks up mode `nav` first while the results have focus, then the base mode.
3. **The hard-coded tables become default binds.** The preview keys go into a `preview` mode, the J/K… fallbacks into the `nav` defaults. "The user's bind wins" then follows from ordinary merge order rather than special cases, and users can rebind preview keys.
4. **Key sequences.** Either a general `KeySequence` trigger kind (`gg`, `gt`, any prefix), or only the `g` prefix moved to the event loop. See the open questions.
5. **Delete the render-side rewrite:** `apply_focus_binds_mode`, `process_results_nav_key`, `action_from_null` and the fallback tables.

The input boxes (prompt, live input, action box) and the sort menu stay as they are. They already route through `InputState` and are not binds.

## Migration, one step at a time

Each step keeps `cargo test` green and is pushed separately.

1. Move focus into `InputState`; the event loop tracks it. No behaviour change.
2. **Shadow mode.** Translate nav binds into the `BindMap` and resolve them in the event loop too, but keep using the render-side result. In debug builds and tests, assert both give the same actions. The existing tests then exercise both paths against each other.
3. Switch to the event-loop result and turn the hard-coded tables into default binds.
4. Delete the render-side code, port the `keys.rs` unit tests to the event loop, and review the bind snapshot change in `test_bind_tables_snapshot`.

Guards: the `keys.rs` tests (~96% mutation score), the PTY `jump` tests, golden snapshots, the bind snapshot, and `regress.py` (the event loop is on the input path).

Size: about 3–5 sessions. It removes roughly 500 lines.

## Risks

- **Ordering across threads.** The event loop must know the focus when the next key arrives. Step 1 makes it the owner of that decision, which is safer than today, where the render loop guesses after the fact.
- **Visible changes.** The help display and bind snapshot will list nav binds that were invisible before. That is intended, but every changed line needs review.
- **Behaviour corners pinned only by tests.** Examples are `last_consumed_nav_key` de-duplication and Esc/Enter leaving fullscreen. The shadow-mode step exists to surface these before anything switches over.

## Decisions (2026-11-07)

1. **Preview keys are configurable.** They become default binds in a `preview` mode that users can override like any other bind.
2. **General key sequences.** A `KeySequence` trigger replaces the hard-coded `g` prefix. A key that begins a bound sequence waits for the next key, with no timeout, as `g` does today. A sequence and a single key can't both start with the same key in the same mode; config validation reports the conflict.
3. **Help lists nav binds under their own heading.**

## Progress

- Step 1 (8e843a4): focus lives in `InputState`.
- Step 2a (da95c02): `NavResolver` resolves nav keys one command at a time.
- Step 2b: `NavKeymap` parses `[nav.binds]` keys into key combinations and sequences. A differential property test against the old string matching found two parsing differences, both fixed: `shift-?`/`?`, and crokey reading `J` as `j`.
- Step 3a (e40fb53): `NavResolver` looks nav binds up by parsed key (`NavKeymap`); `gg`/`gt`/`gb` are key sequences.
- Step 3b (034eed6): preview keys are data (`NavConfig::default_preview_binds`), overridden by `[ui.nav.preview_binds]`. Defaults merge by parsed key (`NavConfig::add_default_bind`), so a user's `Esc` keeps precedence over the default `esc`.
- Step 3c: the event loop resolves nav keys (`EventLoop::set_nav`, `unit`), with the keymaps parsed once at start instead of on every render batch. The render loop publishes what resolution needs (`InputState::publish`: fullscreen and diagram preview, an input box or overlay taking keys, the sort menu) together with the last `RenderCommand::Fence` it has applied. Before it resolves a key, the event loop waits for that fence (at most 250 ms, still answering a pause), so each key sees the state the earlier keys left, even when several arrive in one write. Two changes follow: `,` opens the sort menu (the render-side path opened it and the `SortMenu` action toggled it shut again), and actions sent straight to the render channel (transform output, fm handlers) are no longer read as nav keys.
- Next, step 4: delete the render-side resolver entry points (`apply_focus_binds_mode`, now test-only), drop `last_consumed_nav_key` by resolving each key once, port the `keys.rs` tests to the event loop, and list nav and preview binds under their own heading in help.
