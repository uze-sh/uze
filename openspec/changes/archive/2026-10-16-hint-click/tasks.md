## 1. The widget

- [x] 1.1 Replace `line`/`within`/`named_within`/`entries_within` in `src/ui/widget/hint.rs` with `Hints` (`new(scopes)`, `key`, `named`, `run`, `hovered`, `tone`, `width`, `render(frame, rect) -> Vec<(Rect, Action)>`, and the documented text-only `line()`), measuring by display width
- [x] 1.2 A hint of one key renders one target over its key and name; a run, an unbound action and an entry dropped for width render none
- [x] 1.3 The hovered key's label is drawn lit through a theme token; no caller paints a hint
- [x] 1.4 Widget tests: rects land on the drawn hints, a run is no target, width cut, hover look, unbound action; port the existing hint tests in `src/ui/tests.rs`
- [x] 1.5 `widget::footer::render` and `widget::screen_header::inline` take `Hints` instead of a `Line` and return the targets they drew

## 2. Management modal

- [x] 2.1 `management.rs` footer and header close register `Hit::OfferedAction` per target, hover from `hovered_offer`; drop `close_hovered`'s danger repaint
- [x] 2.2 `view/plugins.rs` tree header uses the widget and drops its hand-painted hover
- [x] 2.3 `release_notes.rs` moves to `Hints::line()` for its body hint
- [x] 2.4 Hint targets are not registered while an overlay is open (`hit.rs` reads any `OfferedAction` as the dialog's answer)
- [x] 2.5 Test: clicking a footer hint's key performs the same `Intent` as pressing it; a click on a hint under an open dialog is not its answer

## 3. Workspace

- [x] 3.1 Add `WorkspaceHit::HintAction(Action)` dispatching through the same `self.act(action, viewport)` arm as `QuickAction`; a press on a hint key is the host's even inside an open surface, the drawer or recent, which otherwise take every press in their area
- [x] 3.2 `extension_view/footer.rs` returns its targets into a new `Rendered` field; `orchestrator/render.rs::render_extension` wraps them in `HintAction` and feeds hover from `model.hovered`
- [x] 3.3 `orchestrator/render/recent.rs` and `agent_support.rs` move to `Hints` (the clearing question via `tone`) and register their targets
- [x] 3.4 Test: in an open code surface, clicking the `esc` of `esc close` reaches the same outcome as pressing `esc`, and hovering it makes it the hovered hint (`esc` rather than `d`: deleting needs a file selected and a confirmation, which proves nothing more about the click)

## 4. Gate

- [x] 4.1 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace` (layering rules included) pass
- [x] 4.2 `openspec validate hint-click --strict` passes
