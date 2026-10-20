//! Detection of other applications that are simultaneously recording and playing audio.

use crate::VoiceResult;

pub trait CallMonitor: Send {
    fn active_calls(&mut self) -> VoiceResult<Vec<String>>;
}

#[derive(Default)]
pub struct MacCallMonitor;

impl CallMonitor for MacCallMonitor {
    fn active_calls(&mut self) -> VoiceResult<Vec<String>> {
        Ok(platform::recording_and_playing()
            .into_iter()
            .filter(|(process, _)| *process != std::process::id() as i32)
            .map(|(_, application)| application)
            .filter(|application| !never_a_call(application))
            .collect())
    }
}

const NOT_CALLS: &[&str] = &[
    "com.electron.wispr-flow",
    "com.apple.CoreSpeech",
    "com.apple.corespeechd",
    "com.apple.SpeechRecognitionCore",
];

fn never_a_call(bundle: &str) -> bool {
    NOT_CALLS.iter().any(|prefix| {
        bundle.len() >= prefix.len() && bundle[..prefix.len()].eq_ignore_ascii_case(prefix)
    })
}

#[cfg(not(target_os = "macos"))]
mod platform {
    pub fn recording_and_playing() -> Vec<(i32, String)> {
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::c_void;

    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyDataSize(
            object: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> i32;
        fn AudioObjectGetPropertyData(
            object: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringGetCString(
            string: *const c_void,
            buffer: *mut u8,
            size: isize,
            encoding: u32,
        ) -> u8;
        fn CFRelease(value: *const c_void);
    }

    const SYSTEM: u32 = 1;
    const fn code(value: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*value)
    }
    const GLOBAL: u32 = code(b"glob");
    const PROCESSES: u32 = code(b"prs#");
    const PID: u32 = code(b"ppid");
    const RECORDING: u32 = code(b"piri");
    const PLAYING: u32 = code(b"piro");
    const BUNDLE: u32 = code(b"pbid");

    fn get<T: Default>(object: u32, selector: u32) -> Option<T> {
        let address = Address {
            selector,
            scope: GLOBAL,
            element: 0,
        };
        let mut value = T::default();
        let mut size = std::mem::size_of::<T>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                &mut value as *mut T as *mut c_void,
            )
        };
        (status == 0).then_some(value)
    }

    pub fn recording_and_playing() -> Vec<(i32, String)> {
        let address = Address {
            selector: PROCESSES,
            scope: GLOBAL,
            element: 0,
        };
        let mut size = 0_u32;
        let status = unsafe {
            AudioObjectGetPropertyDataSize(SYSTEM, &address, 0, std::ptr::null(), &mut size)
        };
        if status != 0 {
            return Vec::new();
        }
        let mut identifiers = vec![0_u32; size as usize / std::mem::size_of::<u32>()];
        let status = unsafe {
            AudioObjectGetPropertyData(
                SYSTEM,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                identifiers.as_mut_ptr() as *mut c_void,
            )
        };
        if status != 0 {
            return Vec::new();
        }
        identifiers.truncate(size as usize / std::mem::size_of::<u32>());
        identifiers
            .into_iter()
            .filter(|identifier| {
                get::<u32>(*identifier, RECORDING).unwrap_or_default() != 0
                    && get::<u32>(*identifier, PLAYING).unwrap_or_default() != 0
            })
            .map(|identifier| {
                let process = get::<i32>(identifier, PID).unwrap_or_default();
                let application = get::<usize>(identifier, BUNDLE)
                    .filter(|pointer| *pointer != 0)
                    .and_then(|pointer| {
                        let string = pointer as *const c_void;
                        let mut buffer = [0_u8; 256];
                        let copied = unsafe {
                            CFStringGetCString(
                                string,
                                buffer.as_mut_ptr(),
                                buffer.len() as isize,
                                0x0800_0100,
                            )
                        };
                        unsafe { CFRelease(string) };
                        (copied != 0).then(|| {
                            let end = buffer.iter().position(|byte| *byte == 0).unwrap_or(0);
                            String::from_utf8_lossy(&buffer[..end]).to_string()
                        })
                    })
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("process {process}"));
                (process, application)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::never_a_call;

    #[test]
    fn known_dictation_and_system_speech_are_not_calls() {
        for identifier in [
            "com.electron.wispr-flow",
            "com.electron.wispr-flow.helper",
            "com.apple.CoreSpeech",
            "com.apple.corespeechd",
        ] {
            assert!(never_a_call(identifier), "{identifier}");
        }
        for identifier in [
            "us.zoom.xos",
            "com.apple.FaceTime",
            "com.tinyspeck.slackmacgap",
        ] {
            assert!(!never_a_call(identifier), "{identifier}");
        }
    }
}
