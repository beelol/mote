use std::sync::{Mutex, OnceLock};

static CONTEXT: OnceLock<egui::Context> = OnceLock::new();
static RETURN: Mutex<Option<u64>> = Mutex::new(None);

#[cfg(target_os = "ios")]
extern "C" {
    fn mote_ios_text_input_publish(bytes: *const u8, length: usize);
}

pub fn install(ctx: &egui::Context) {
    let _ = CONTEXT.set(ctx.clone());
}

#[no_mangle]
pub extern "C" fn mote_ios_text_input_update(
    id: u64,
    bytes: *const u8,
    length: usize,
    selection_start: usize,
    selection_end: usize,
) {
    if length > 256 * 1024 || (bytes.is_null() && length != 0) {
        return;
    }
    let bytes = if length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(bytes, length) }
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };
    mote_client_core::native_text::queue_update(
        id,
        text.to_owned(),
        selection_start,
        selection_end,
    );
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint();
    }
}

#[no_mangle]
pub extern "C" fn mote_ios_text_input_return(id: u64) {
    if let Ok(mut pending) = RETURN.lock() {
        *pending = Some(id);
    }
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint();
    }
}

pub fn apply_pending(ctx: &egui::Context) {
    let pending = match RETURN.lock() {
        Ok(mut pending) => pending.take(),
        Err(_) => return,
    };
    let Some(id) = pending else {
        return;
    };
    if ctx.memory(|memory| memory.focused().map(|focused| focused.value())) == Some(id) {
        ctx.input_mut(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        });
    }
}

#[cfg(target_os = "ios")]
pub fn publish() {
    match mote_client_core::native_text::take_frame() {
        Some(input) => {
            if let Ok(data) = serde_json::to_vec(&input) {
                unsafe { mote_ios_text_input_publish(data.as_ptr(), data.len()) };
            }
        }
        None => unsafe { mote_ios_text_input_publish(std::ptr::null(), 0) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn native_callback_rejects_invalid_utf8_and_applies_valid_updates() {
        let _guard = TEST_LOCK.lock().unwrap();
        let ctx = egui::Context::default();
        install(&ctx);
        let id = egui::Id::new("callback-field");
        let invalid = [0xff];
        mote_ios_text_input_update(id.value(), invalid.as_ptr(), invalid.len(), 0, 0);
        let mut text = "unchanged".to_owned();
        assert!(!mote_client_core::native_text::apply_pending(
            &ctx, id, &mut text
        ));

        let valid = "native".as_bytes();
        mote_ios_text_input_update(id.value(), valid.as_ptr(), valid.len(), 2, 4);
        assert!(mote_client_core::native_text::apply_pending(
            &ctx, id, &mut text
        ));
        assert_eq!(text, "native");
    }

    #[test]
    fn native_callback_accepts_an_empty_field_with_a_null_buffer() {
        let _guard = TEST_LOCK.lock().unwrap();
        let ctx = egui::Context::default();
        let id = egui::Id::new("cleared-field");
        mote_ios_text_input_update(id.value(), std::ptr::null(), 0, 0, 0);

        let mut text = "remove me".to_owned();
        assert!(mote_client_core::native_text::apply_pending(
            &ctx, id, &mut text
        ));
        assert!(text.is_empty());
    }

    #[test]
    fn native_return_becomes_an_egui_enter_event() {
        let _guard = TEST_LOCK.lock().unwrap();
        let ctx = egui::Context::default();
        let id = egui::Id::new("return-field");
        ctx.memory_mut(|memory| memory.request_focus(id));
        mote_ios_text_input_return(id.value());
        let mut pressed = false;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            apply_pending(ctx);
            pressed = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        });
        assert!(pressed);
    }

    #[test]
    fn delayed_native_return_does_not_target_a_new_field() {
        let _guard = TEST_LOCK.lock().unwrap();
        let ctx = egui::Context::default();
        let original = egui::Id::new("original-field");
        let replacement = egui::Id::new("replacement-field");
        ctx.memory_mut(|memory| memory.request_focus(replacement));
        mote_ios_text_input_return(original.value());

        let mut pressed = false;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            apply_pending(ctx);
            pressed = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        });
        assert!(!pressed);
    }
}
