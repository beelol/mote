#[cfg(test)]
std::thread_local! {
    static SYSTEM_STATE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

#[cfg(any(test, target_os = "ios"))]
fn system_appearance() -> Option<egui::Theme> {
    #[cfg(test)]
    let state = SYSTEM_STATE.get();
    #[cfg(all(not(test), target_os = "ios"))]
    let state = unsafe { mote_ios_appearance() };
    #[cfg(all(not(test), not(target_os = "ios")))]
    let state = 0;

    match state {
        1 => Some(egui::Theme::Light),
        2 => Some(egui::Theme::Dark),
        _ => None,
    }
}

#[cfg(any(test, target_os = "ios"))]
pub(crate) fn sync(ctx: &egui::Context) {
    if let Some(theme) = system_appearance() {
        ctx.set_theme(match theme {
            egui::Theme::Light => egui::ThemePreference::Light,
            egui::Theme::Dark => egui::ThemePreference::Dark,
        });
    }
}

#[cfg(any(test, target_os = "ios"))]
static CONTEXT: std::sync::Mutex<Option<egui::Context>> = std::sync::Mutex::new(None);

#[cfg(target_os = "ios")]
extern "C" {
    fn mote_ios_appearance() -> u8;
    fn mote_ios_appearance_observe(changed: extern "C" fn()) -> *mut std::ffi::c_void;
    fn mote_ios_appearance_remove(observer: *mut std::ffi::c_void);
}

#[cfg(any(test, target_os = "ios"))]
pub(crate) struct Bridge {
    #[cfg(target_os = "ios")]
    observer: Option<std::ptr::NonNull<std::ffi::c_void>>,
}

#[cfg(any(test, target_os = "ios"))]
impl Bridge {
    pub(crate) fn install(ctx: &egui::Context) -> Self {
        sync(ctx);
        *CONTEXT.lock().unwrap_or_else(|error| error.into_inner()) = Some(ctx.clone());
        Self {
            #[cfg(target_os = "ios")]
            observer: std::ptr::NonNull::new(unsafe {
                mote_ios_appearance_observe(mote_ios_appearance_changed)
            }),
        }
    }
}

#[cfg(any(test, target_os = "ios"))]
impl Drop for Bridge {
    fn drop(&mut self) {
        *CONTEXT.lock().unwrap_or_else(|error| error.into_inner()) = None;
        #[cfg(target_os = "ios")]
        if let Some(observer) = self.observer.take() {
            unsafe { mote_ios_appearance_remove(observer.as_ptr()) };
        }
    }
}

#[cfg(any(test, target_os = "ios"))]
extern "C" fn mote_ios_appearance_changed() {
    let ctx = CONTEXT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    if let Some(ctx) = ctx {
        sync(&ctx);
        ctx.request_repaint();
    }
}

#[cfg(test)]
fn set_test_system_state(state: u8) {
    SYSTEM_STATE.set(state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_light_and_dark_traits_select_the_matching_installed_vector() {
        let ctx = egui::Context::default();
        mote_client_core::install_theme(&ctx);

        set_test_system_state(1);
        sync(&ctx);
        assert_eq!(
            ctx.style().visuals.panel_fill,
            egui::Color32::from_rgb(244, 241, 234)
        );

        set_test_system_state(2);
        sync(&ctx);
        assert_eq!(
            ctx.style().visuals.panel_fill,
            egui::Color32::from_rgb(11, 11, 12)
        );
    }

    #[test]
    fn trait_changes_sync_theme_and_request_repaint_only_while_installed() {
        let ctx = egui::Context::default();
        mote_client_core::install_theme(&ctx);
        set_test_system_state(1);
        let bridge = Bridge::install(&ctx);
        settle(&ctx);

        set_test_system_state(2);
        mote_ios_appearance_changed();
        assert_eq!(
            ctx.style().visuals.panel_fill,
            egui::Color32::from_rgb(11, 11, 12)
        );
        assert!(ctx.has_requested_repaint());

        settle(&ctx);
        drop(bridge);
        set_test_system_state(1);
        mote_ios_appearance_changed();
        assert_eq!(
            ctx.style().visuals.panel_fill,
            egui::Color32::from_rgb(11, 11, 12)
        );
        assert!(!ctx.has_requested_repaint());
    }

    fn settle(ctx: &egui::Context) {
        for _ in 0..4 {
            let _ = ctx.run(egui::RawInput::default(), |_| {});
        }
        assert!(!ctx.has_requested_repaint());
    }
}
