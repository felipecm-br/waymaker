//! Mouse handling inside the render loop: clicks, drags, scrolling and hover.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use log::debug;
use ratatui::layout::{Position, Rect};

use super::{Click, Flow, State, find_interaction};
use crate::ui::{DisplayUI, PickerUI, PreviewUI};
use crate::{SSS, Selection};

/// The render loop's state that mouse handling reads or mutates.
pub(super) struct MouseCtx<'r, 'a, T: SSS, S: Selection> {
    pub state: &'r mut State,
    pub picker_ui: &'r mut PickerUI<'a, T, S>,
    pub footer_ui: &'r DisplayUI,
    pub preview_ui: &'r mut Option<PreviewUI>,
    pub click: &'r mut Click,
    pub mouse_hover: &'r mut Option<Position>,
    pub did_cursor_wrap: &'r mut bool,
}

pub(super) fn handle_mouse<T: SSS, S: Selection>(
    mouse: MouseEvent,
    cx: MouseCtx<'_, '_, T, S>,
) -> Flow<S> {
    let MouseCtx {
        state,
        picker_ui,
        footer_ui,
        preview_ui,
        click,
        mouse_hover,
        did_cursor_wrap,
    } = cx;

    use crate::config::Side;
    // we could also impl this in the aliasing step
    let pos = Position::from((mouse.column, mouse.row));
    let layout = state.layout;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(p) = preview_ui.as_mut()
                && p.visible()
            {
                let gap_rect = layout.gap;
                let is_in_drag_area = if !gap_rect.is_empty() {
                    // Dedicated gap area: click anywhere in the gap to start drag.
                    gap_rect.contains(pos)
                } else {
                    // Fallback: use the legacy drag_width border-edge approach.
                    let drag_width = p.drag_width();
                    if drag_width > 0 {
                        let side = p.setting().map(|s| &s.layout.side).unwrap_or(&Side::Right);
                        drag_edge(layout.preview, side, drag_width).contains(pos)
                    } else {
                        false
                    }
                };

                if is_in_drag_area {
                    state.dragging = Some(pos);
                    return Flow::Continue;
                }
            }

            if layout.results.contains(pos) {
                let y = mouse.row - layout.results.top();
                debug!("Results clicked at: {y}");
                *click = Click::ResultPos(y);
            } else if layout.input.contains(pos) {
                // The X offset of the start of the visible text relative to the terminal
                let text_start_x = layout.input.x + picker_ui.query.left();

                if pos.x >= text_start_x {
                    let visual_offset = pos.x - text_start_x;
                    picker_ui.query.set_at_visual_offset(visual_offset);
                } else {
                    picker_ui.query.set(None, 0);
                }
            } else if layout.status.contains(pos) {
                let x = pos.x.saturating_sub(layout.status.x);
                debug!("Status clicked at x: {x}");
                if let Some(action) =
                    find_interaction(&picker_ui.results.status_config.interactions, x)
                {
                    *click = Click::Semantic(action);
                }
            } else if layout.header.contains(pos) {
                let rel_x = pos.x.saturating_sub(layout.header.x);
                let rel_y = pos.y.saturating_sub(layout.header.y);
                debug!("Header clicked at x: {rel_x}, y: {rel_y}");

                if let Some(setting) = picker_ui.header.config.interactions.get(rel_y as usize)
                    && let Some(action) = find_interaction(setting, rel_x)
                {
                    *click = Click::Semantic(action);
                }
            } else if layout.footer.contains(pos) {
                let rel_x = pos.x.saturating_sub(layout.footer.x);
                let rel_y = pos.y.saturating_sub(layout.footer.y);
                debug!("Footer clicked at x: {rel_x}, y: {rel_y}");

                if let Some(setting) = footer_ui.config.interactions.get(rel_y as usize)
                    && let Some(action) = find_interaction(setting, rel_x)
                {
                    *click = Click::Semantic(action);
                }
            }
        }
        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
            if layout.preview.contains(pos) {
                if let Some(p) = preview_ui.as_mut() {
                    if matches!(mouse.kind, MouseEventKind::ScrollDown) {
                        p.down(1)
                    } else {
                        p.up(1)
                    }
                }
            } else {
                let next =
                    matches!(mouse.kind, MouseEventKind::ScrollDown) ^ picker_ui.results.reverse();
                *did_cursor_wrap = if next {
                    picker_ui.results.cursor_next()
                } else {
                    picker_ui.results.cursor_prev()
                };
            }
        }
        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
            let left = matches!(mouse.kind, MouseEventKind::ScrollLeft);
            if layout.preview.contains(pos) {
                if let Some(p) = preview_ui.as_mut() {
                    p.scroll(true, if left { -1 } else { 1 })
                }
            } else {
                if !left || picker_ui.results.hscroll > 0 || !picker_ui.query.input.is_empty() {
                    picker_ui
                        .results
                        .current_scroll(if left { -1 } else { 1 }, true);
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(start_pos) = state.dragging
                && let Some(p) = preview_ui.as_mut()
            {
                let side = p.setting().map(|s| &s.layout.side).unwrap_or(&Side::Right);
                match drag_resize(side, start_pos, pos) {
                    Some(Ok(n)) => p.expand(n),
                    Some(Err(n)) => p.shrink(n),
                    None => {}
                }
                state.dragging = Some(pos);
                state.needs_redraw = true;
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            state.needs_redraw |= state.dragging.take().is_some();
        }
        MouseEventKind::Moved => {
            let prev_hover = *mouse_hover;
            *mouse_hover = Some(pos);
            let gap_rect = layout.gap;
            let was_over_gap = prev_hover.is_some_and(|p| gap_rect.contains(p));
            let is_over_gap = gap_rect.contains(pos);
            if was_over_gap != is_over_gap {
                state.needs_redraw = true;
            }
        }
        _ => {}
    }

    Flow::Proceed
}

/// The strip of the preview, `drag_width` deep along the edge facing the picker, where a click
/// starts a resize drag (used when the layout has no gap column).
fn drag_edge(preview: Rect, side: &crate::config::Side, drag_width: u16) -> Rect {
    use crate::config::Side;
    match side {
        Side::Right => Rect {
            x: preview.x,
            y: preview.y,
            width: drag_width,
            height: preview.height,
        },
        Side::Left => Rect {
            x: preview.x + preview.width.saturating_sub(drag_width),
            y: preview.y,
            width: drag_width,
            height: preview.height,
        },
        Side::Bottom => Rect {
            x: preview.x,
            y: preview.y,
            width: preview.width,
            height: drag_width,
        },
        Side::Top => Rect {
            x: preview.x,
            y: preview.y + preview.height.saturating_sub(drag_width),
            width: preview.width,
            height: drag_width,
        },
    }
}

/// How a drag from `start` to `pos` resizes a preview on `side`: `Ok(n)` grows it by `n`,
/// `Err(n)` shrinks it, `None` leaves it (no movement along the resize axis). Moving the edge
/// towards the picker grows the preview.
fn drag_resize(
    side: &crate::config::Side,
    start: Position,
    pos: Position,
) -> Option<Result<u16, u16>> {
    use crate::config::Side;
    let (from, to, grow_when_less) = match side {
        Side::Right => (start.x, pos.x, true),
        Side::Left => (start.x, pos.x, false),
        Side::Bottom => (start.y, pos.y, true),
        Side::Top => (start.y, pos.y, false),
    };
    if to == from {
        return None;
    }
    let delta = from.abs_diff(to);
    Some(if (to < from) == grow_when_less {
        Ok(delta)
    } else {
        Err(delta)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Side;

    #[test]
    fn drag_edge_is_the_preview_side_facing_the_picker() {
        let preview = Rect::new(10, 5, 40, 20);
        assert_eq!(drag_edge(preview, &Side::Right, 2), Rect::new(10, 5, 2, 20));
        assert_eq!(drag_edge(preview, &Side::Left, 2), Rect::new(48, 5, 2, 20));
        assert_eq!(
            drag_edge(preview, &Side::Bottom, 1),
            Rect::new(10, 5, 40, 1)
        );
        assert_eq!(drag_edge(preview, &Side::Top, 1), Rect::new(10, 24, 40, 1));
    }

    #[test]
    fn dragging_the_edge_towards_the_picker_grows_the_preview() {
        let at = |x, y| Position::new(x, y);
        let start = at(20, 10);
        // Right-side preview: its edge is on the left; dragging left grows it.
        assert_eq!(drag_resize(&Side::Right, start, at(17, 10)), Some(Ok(3)));
        assert_eq!(drag_resize(&Side::Right, start, at(24, 10)), Some(Err(4)));
        assert_eq!(drag_resize(&Side::Left, start, at(25, 10)), Some(Ok(5)));
        assert_eq!(drag_resize(&Side::Left, start, at(18, 10)), Some(Err(2)));
        assert_eq!(drag_resize(&Side::Bottom, start, at(20, 7)), Some(Ok(3)));
        assert_eq!(drag_resize(&Side::Top, start, at(20, 12)), Some(Ok(2)));
        assert_eq!(drag_resize(&Side::Top, start, at(20, 9)), Some(Err(1)));
        // Movement across the resize axis does nothing.
        assert_eq!(drag_resize(&Side::Right, start, at(20, 3)), None);
        assert_eq!(drag_resize(&Side::Bottom, start, at(1, 10)), None);
    }
}
