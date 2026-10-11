mod app;

#[no_mangle]
pub extern "C" fn mote_ios_main() -> ! {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        run_and_return: false,
        ..Default::default()
    };
    if let Err(error) = eframe::run_native(
        "Mote",
        options,
        Box::new(|cc| {
            #[cfg(target_os = "ios")]
            attach_scene(cc)?;
            mote_client_core::install_theme(&cc.egui_ctx);
            Ok(Box::new(app::MoteApp::default()))
        }),
    ) {
        eprintln!("Mote could not start: {error}");
    }
    std::process::exit(1)
}

#[cfg(target_os = "ios")]
fn attach_scene(cc: &eframe::CreationContext<'_>) -> Result<(), raw_window_handle::HandleError> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    extern "C" {
        fn mote_ios_attach_view(view: *mut std::ffi::c_void);
    }
    let RawWindowHandle::UiKit(handle) = cc.window_handle()?.as_raw() else {
        return Err(raw_window_handle::HandleError::NotSupported);
    };
    // Winit owns this live UIView. The native adapter only attaches its window to the scene.
    unsafe { mote_ios_attach_view(handle.ui_view.as_ptr()) };
    Ok(())
}

fn safe_area() -> egui::Margin {
    #[cfg(target_os = "ios")]
    {
        #[repr(C)]
        struct Insets {
            top: f64,
            left: f64,
            bottom: f64,
            right: f64,
        }
        extern "C" {
            fn mote_ios_safe_area() -> Insets;
        }
        // Eframe calls update on UIKit's main thread, which owns the window.
        let insets = unsafe { mote_ios_safe_area() };
        egui::Margin {
            top: insets.top.ceil() as i8,
            left: insets.left.ceil() as i8,
            bottom: insets.bottom.ceil() as i8,
            right: insets.right.ceil() as i8,
        }
    }
    #[cfg(not(target_os = "ios"))]
    egui::Margin::ZERO
}
