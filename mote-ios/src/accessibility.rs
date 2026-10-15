use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy)]
struct PendingAction {
    id: u64,
    kind: u8,
}

static ACTIONS: Mutex<Vec<PendingAction>> = Mutex::new(Vec::new());
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();

#[cfg(target_os = "ios")]
extern "C" {
    fn mote_ios_accessibility_publish(bytes: *const u8, length: usize);
}

pub fn install(ctx: &egui::Context) {
    let _ = CONTEXT.set(ctx.clone());
}

#[no_mangle]
pub extern "C" fn mote_ios_accessibility_action(id: u64, kind: u8) {
    if kind > 3 {
        return;
    }
    if let Ok(mut actions) = ACTIONS.lock() {
        actions.push(PendingAction { id, kind });
    }
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint();
    }
}

pub fn apply_pending(ctx: &egui::Context) {
    let actions = match ACTIONS.lock() {
        Ok(mut queued) => std::mem::take(&mut *queued),
        Err(_) => return,
    };
    for pending in actions {
        if pending.kind == 3 {
            mote_client_core::native_text::queue_focus(pending.id);
            continue;
        }
        let action = match pending.kind {
            0 => accesskit::Action::Click,
            1 => accesskit::Action::Increment,
            2 => accesskit::Action::Decrement,
            _ => continue,
        };
        ctx.input_mut(|input| {
            input.events.push(egui::Event::AccessKitActionRequest(
                accesskit::ActionRequest {
                    action,
                    target: pending.id.into(),
                    data: None,
                },
            ));
        });
    }
}

#[cfg(target_os = "ios")]
pub fn publish() {
    let frame = mote_client_core::native_accessibility::take_frame();
    if let Ok(data) = serde_json::to_vec(&frame) {
        unsafe { mote_ios_accessibility_publish(data.as_ptr(), data.len()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn clear_actions() {
        ACTIONS.lock().unwrap().clear();
    }

    #[test]
    fn native_click_is_consumed_once_by_the_matching_egui_control() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear_actions();
        let ctx = egui::Context::default();
        install(&ctx);
        let id = egui::Id::new("native-action-test");
        mote_ios_accessibility_action(id.value(), 0);
        mote_ios_accessibility_action(id.value(), 0);
        mote_ios_accessibility_action(id.value(), 9);
        let mut clicks = 0;
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                apply_pending(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    if ui
                        .interact(
                            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(48.0, 48.0)),
                            id,
                            egui::Sense::click(),
                        )
                        .clicked()
                    {
                        clicks += 1;
                    }
                });
            });
        }
        assert_eq!(clicks, 1);
    }

    #[test]
    fn native_text_focus_targets_any_generated_field_id() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear_actions();
        let ctx = egui::Context::default();
        install(&ctx);
        let id = egui::Id::new(("generated-field", vec![2_usize, 4]));
        mote_ios_accessibility_action(id.value(), 3);

        let mut text = String::new();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            apply_pending(ctx);
            egui::CentralPanel::default().show(ctx, |ui| {
                let response = mote_client_core::native_text::singleline(
                    ui,
                    id,
                    &mut text,
                    egui::vec2(200.0, 48.0),
                    "Generated field",
                    false,
                );
                assert!(response.has_focus());
            });
        });
    }
}
