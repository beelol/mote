#[cfg(test)]
std::thread_local! {
    static SYSTEM_STATE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    static SYSTEM_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) fn system_reduced_motion() -> bool {
    #[cfg(test)]
    {
        SYSTEM_READS.set(SYSTEM_READS.get() + 1);
        SYSTEM_STATE.get() != 0
    }
    #[cfg(all(not(test), target_os = "ios"))]
    return unsafe { mote_ios_reduce_motion_enabled() != 0 };
    #[cfg(all(not(test), not(target_os = "ios")))]
    false
}

#[cfg(any(test, target_os = "ios"))]
static CONTEXT: std::sync::Mutex<Option<egui::Context>> = std::sync::Mutex::new(None);

#[cfg(target_os = "ios")]
extern "C" {
    fn mote_ios_reduce_motion_enabled() -> u8;
    fn mote_ios_motion_observe(changed: extern "C" fn()) -> *mut std::ffi::c_void;
    fn mote_ios_motion_remove(observer: *mut std::ffi::c_void);
}

#[cfg(any(test, target_os = "ios"))]
pub(crate) struct Bridge {
    #[cfg(target_os = "ios")]
    observer: Option<std::ptr::NonNull<std::ffi::c_void>>,
}

#[cfg(any(test, target_os = "ios"))]
impl Bridge {
    pub(crate) fn install(ctx: &egui::Context) -> Self {
        *CONTEXT.lock().unwrap_or_else(|error| error.into_inner()) = Some(ctx.clone());
        Self {
            #[cfg(target_os = "ios")]
            observer: std::ptr::NonNull::new(unsafe {
                mote_ios_motion_observe(mote_ios_motion_changed)
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
            unsafe { mote_ios_motion_remove(observer.as_ptr()) };
        }
    }
}

#[cfg(any(test, target_os = "ios"))]
extern "C" fn mote_ios_motion_changed() {
    let ctx = CONTEXT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    if let Some(ctx) = ctx {
        ctx.request_repaint();
    }
}

#[cfg(test)]
pub(crate) fn set_test_system_state(state: u8) {
    SYSTEM_STATE.set(state);
    SYSTEM_READS.set(0);
}

#[cfg(test)]
pub(crate) fn test_system_reads() -> usize {
    SYSTEM_READS.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_preference_is_read_again_and_unknown_values_reduce_motion() {
        for state in [0, 1, 0, 2, u8::MAX, 0] {
            set_test_system_state(state);
            assert_eq!(system_reduced_motion(), state != 0, "native state {state}");
        }
    }

    #[test]
    fn motion_notifications_request_repaint_only_during_bridge_lifetime() {
        let ctx = egui::Context::default();
        let settle = || {
            for _ in 0..4 {
                let _ = ctx.run(egui::RawInput::default(), |_| {});
            }
            assert!(!ctx.has_requested_repaint());
        };
        settle();
        for _ in 0..3 {
            let bridge = Bridge::install(&ctx);
            for _ in 0..3 {
                mote_ios_motion_changed();
                assert!(ctx.has_requested_repaint());
                settle();
            }
            drop(bridge);
            mote_ios_motion_changed();
            assert!(!ctx.has_requested_repaint());
        }
    }
}
