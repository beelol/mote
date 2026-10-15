use egui::text::{CCursor, CCursorRange};
use serde::Serialize;
use std::{cell::RefCell, sync::Mutex};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ActiveTextInput {
    pub id: u64,
    pub label: String,
    pub text: String,
    pub selection_start: usize,
    pub selection_end: usize,
    pub rect: [f32; 4],
    pub secure: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct NativeUpdate {
    id: u64,
    text: String,
    selection_start: usize,
    selection_end: usize,
}

thread_local! {
    static FRAME: RefCell<Option<ActiveTextInput>> = const { RefCell::new(None) };
}

static UPDATE: Mutex<Option<NativeUpdate>> = Mutex::new(None);
static FOCUS_REQUEST: Mutex<Option<u64>> = Mutex::new(None);

pub fn begin_frame() {
    FRAME.with(|frame| *frame.borrow_mut() = None);
}

pub fn publish(input: ActiveTextInput) {
    FRAME.with(|frame| *frame.borrow_mut() = Some(input));
}

pub fn take_frame() -> Option<ActiveTextInput> {
    if let Ok(mut update) = UPDATE.lock() {
        *update = None;
    }
    if let Ok(mut request) = FOCUS_REQUEST.lock() {
        *request = None;
    }
    FRAME.with(|frame| frame.borrow_mut().take())
}

pub fn queue_update(id: u64, text: String, selection_start: usize, selection_end: usize) {
    if text.len() > 256 * 1024 {
        return;
    }
    if let Ok(mut update) = UPDATE.lock() {
        *update = Some(NativeUpdate {
            id,
            text,
            selection_start,
            selection_end,
        });
    }
}

pub fn queue_focus(id: u64) {
    if let Ok(mut request) = FOCUS_REQUEST.lock() {
        *request = Some(id);
    }
}

fn take_focus(id: u64) -> bool {
    let Ok(mut request) = FOCUS_REQUEST.lock() else {
        return false;
    };
    if request.as_ref() == Some(&id) {
        *request = None;
        true
    } else {
        false
    }
}

pub fn apply_pending(ctx: &egui::Context, id: egui::Id, text: &mut String) -> bool {
    let update = {
        let Ok(mut update) = UPDATE.lock() else {
            return false;
        };
        if update.as_ref().map(|update| update.id) != Some(id.value()) {
            return false;
        }
        update.take().expect("matching update must exist")
    };
    *text = update.text;
    let start = utf16_offset_to_char(text, update.selection_start.min(update.selection_end));
    let end = utf16_offset_to_char(text, update.selection_start.max(update.selection_end));
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(start),
        CCursor::new(end),
    )));
    egui::TextEdit::store_state(ctx, id, state);
    true
}

pub fn full_hit_rect(visual: egui::Rect, right_edge: f32) -> egui::Rect {
    let half_height = visual.height().max(48.0) / 2.0;
    egui::Rect::from_min_max(
        egui::pos2(visual.left(), visual.center().y - half_height),
        egui::pos2(
            visual.right().max(right_edge),
            visual.center().y + half_height,
        ),
    )
}

pub fn singleline(
    ui: &mut egui::Ui,
    id: egui::Id,
    text: &mut String,
    size: egui::Vec2,
    hint: &str,
    secure: bool,
) -> egui::Response {
    if take_focus(id.value()) {
        ui.memory_mut(|memory| memory.request_focus(id));
    }
    let native_changed = apply_pending(ui.ctx(), id, text);
    let mut edit = egui::TextEdit::singleline(text).id(id).hint_text(hint);
    if secure {
        edit = edit.password(true);
    }
    let mut response = ui.add_sized(size, edit);
    let hit_rect = full_hit_rect(response.rect, response.rect.right());
    let hit = ui.interact(hit_rect, id, response.sense);
    response = response.union(hit);
    if native_changed {
        response.mark_changed();
    }
    if response.has_focus() {
        let range = egui::TextEdit::load_state(ui.ctx(), id)
            .and_then(|state| state.cursor.char_range())
            .unwrap_or_else(|| CCursorRange::one(CCursor::new(text.chars().count())));
        publish(ActiveTextInput {
            id: id.value(),
            label: hint.to_owned(),
            text: text.clone(),
            selection_start: char_to_utf16_offset(text, range.secondary.index),
            selection_end: char_to_utf16_offset(text, range.primary.index),
            rect: [
                hit_rect.left(),
                hit_rect.top(),
                hit_rect.width(),
                hit_rect.height(),
            ],
            secure,
        });
    }
    response
}

fn utf16_offset_to_char(text: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    let mut chars = 0;
    for character in text.chars() {
        if utf16 >= offset {
            break;
        }
        let next = utf16 + character.len_utf16();
        if offset < next {
            break;
        }
        utf16 = next;
        chars += 1;
    }
    chars
}

fn char_to_utf16_offset(text: &str, index: usize) -> usize {
    text.chars().take(index).map(char::len_utf16).sum()
}

#[cfg(test)]
fn clear() {
    begin_frame();
    *UPDATE.lock().unwrap() = None;
    *FOCUS_REQUEST.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn native_update_targets_one_field_and_preserves_unicode_selection() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear();
        let ctx = egui::Context::default();
        let target = egui::Id::new("target");
        let other = egui::Id::new("other");
        queue_update(target.value(), "Hi 🫡".into(), 3, 5);

        let mut untouched = "other".to_owned();
        assert!(!apply_pending(&ctx, other, &mut untouched));
        assert_eq!(untouched, "other");

        let mut text = String::new();
        assert!(apply_pending(&ctx, target, &mut text));
        assert_eq!(text, "Hi 🫡");
        let range = egui::TextEdit::load_state(&ctx, target)
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        assert_eq!(range.secondary.index, 3);
        assert_eq!(range.primary.index, 4);
    }

    #[test]
    fn native_update_normalizes_reversed_selection() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear();
        let ctx = egui::Context::default();
        let id = egui::Id::new("reversed-selection");
        queue_update(id.value(), "hello".into(), 4, 1);

        let mut text = String::new();
        assert!(apply_pending(&ctx, id, &mut text));
        let range = egui::TextEdit::load_state(&ctx, id)
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        assert_eq!((range.secondary.index, range.primary.index), (1, 4));
    }

    #[test]
    fn active_field_frame_replaces_stale_state() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear();
        begin_frame();
        publish(ActiveTextInput {
            id: 7,
            label: "Greeting".into(),
            text: "hello".into(),
            selection_start: 1,
            selection_end: 4,
            rect: [10.0, 20.0, 200.0, 48.0],
            secure: false,
        });
        assert_eq!(take_frame().unwrap().id, 7);
        begin_frame();
        assert!(take_frame().is_none());
    }

    #[test]
    fn end_of_frame_expires_unconsumed_native_state() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear();
        let ctx = egui::Context::default();
        let stale = egui::Id::new("removed-field");
        queue_update(stale.value(), "stale".into(), 0, 5);
        queue_focus(stale.value());

        assert!(take_frame().is_none());

        let mut text = "current".to_owned();
        assert!(!apply_pending(&ctx, stale, &mut text));
        assert!(!take_focus(stale.value()));
        assert_eq!(text, "current");
    }

    #[test]
    fn full_hit_rect_includes_the_visual_field_and_minimum_touch_height() {
        let _guard = TEST_LOCK.lock().unwrap();
        let visual = egui::Rect::from_min_size(egui::pos2(20.0, 30.0), egui::vec2(280.0, 36.0));
        let hit = full_hit_rect(visual, 320.0);
        assert!(hit.contains(visual.left_top()));
        assert!(hit.contains(visual.right_bottom()));
        assert_eq!(hit.height(), 48.0);
        assert_eq!(hit.left(), 20.0);
        assert_eq!(hit.right(), 320.0);
    }

    #[test]
    fn singleline_applies_native_edits_and_publishes_focused_state() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear();
        let ctx = egui::Context::default();
        let id = egui::Id::new("native-field");
        let mut text = "before".to_owned();
        queue_update(id.value(), "after".into(), 1, 4);
        ctx.memory_mut(|memory| memory.request_focus(id));

        let mut changed = false;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            begin_frame();
            egui::CentralPanel::default().show(ctx, |ui| {
                changed = singleline(ui, id, &mut text, egui::vec2(280.0, 48.0), "Field", false)
                    .changed();
            });
        });

        assert!(changed);
        assert_eq!(text, "after");
        let active = take_frame().unwrap();
        assert_eq!(active.id, id.value());
        assert_eq!(active.label, "Field");
        assert_eq!((active.selection_start, active.selection_end), (1, 4));
        assert_eq!(active.rect[3], 48.0);
    }
}
