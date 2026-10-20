//! macOS system speech rendered through AudioToolbox callbacks into memory.

use crate::VoiceResult;

#[cfg(not(target_os = "macos"))]
use crate::VoiceError;

#[cfg(not(target_os = "macos"))]
pub fn speak(
    _text: &str,
    _voice: Option<&str>,
    _words_per_minute: Option<u32>,
) -> VoiceResult<Vec<f32>> {
    Err(VoiceError::Unavailable(
        "system speech synthesis is available on macOS only",
    ))
}

#[cfg(target_os = "macos")]
pub fn speak(
    text: &str,
    voice: Option<&str>,
    words_per_minute: Option<u32>,
) -> VoiceResult<Vec<f32>> {
    platform::speak(text, voice, words_per_minute)
}

#[cfg(target_os = "macos")]
mod platform {
    #![allow(non_upper_case_globals, non_snake_case)]

    use crate::{VoiceError, VoiceResult};
    use std::ffi::c_void;
    use std::time::{Duration, Instant};

    type OSStatus = i32;
    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFNumberRef = *const c_void;
    type AudioFileId = *mut c_void;
    type ExtAudioFileRef = *mut c_void;
    type SpeechChannel = *mut c_void;

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct AudioDescription {
        sample_rate: f64,
        format_id: u32,
        format_flags: u32,
        bytes_per_packet: u32,
        frames_per_packet: u32,
        bytes_per_frame: u32,
        channels_per_frame: u32,
        bits_per_channel: u32,
        reserved: u32,
    }

    #[repr(C, packed(2))]
    #[derive(Default, Clone, Copy)]
    struct VoiceSpec {
        creator: u32,
        id: u32,
    }

    #[repr(C, packed(2))]
    #[derive(Clone, Copy)]
    struct VoiceDescription {
        length: i32,
        voice: VoiceSpec,
        version: u32,
        name: [u8; 64],
        comment: [u8; 256],
        gender: i16,
        age: i16,
        script: i16,
        language: i16,
        region: i16,
        reserved: [i32; 4],
    }

    type ReadCallback = extern "C" fn(*mut c_void, i64, u32, *mut c_void, *mut u32) -> OSStatus;
    type WriteCallback = extern "C" fn(*mut c_void, i64, u32, *const c_void, *mut u32) -> OSStatus;
    type GetSizeCallback = extern "C" fn(*mut c_void) -> i64;
    type SetSizeCallback = extern "C" fn(*mut c_void, i64) -> OSStatus;

    #[link(name = "AudioToolbox", kind = "framework")]
    extern "C" {
        fn AudioFileInitializeWithCallbacks(
            client: *mut c_void,
            read: ReadCallback,
            write: WriteCallback,
            get_size: GetSizeCallback,
            set_size: SetSizeCallback,
            file_type: u32,
            format: *const AudioDescription,
            flags: u32,
            output: *mut AudioFileId,
        ) -> OSStatus;
        fn AudioFileClose(file: AudioFileId) -> OSStatus;
        fn ExtAudioFileWrapAudioFileID(
            file: AudioFileId,
            for_writing: u8,
            output: *mut ExtAudioFileRef,
        ) -> OSStatus;
        fn ExtAudioFileDispose(file: ExtAudioFileRef) -> OSStatus;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        static kSpeechOutputToExtAudioFileProperty: CFStringRef;
        static kSpeechRateProperty: CFStringRef;
        fn NewSpeechChannel(voice: *const VoiceSpec, output: *mut SpeechChannel) -> OSStatus;
        fn DisposeSpeechChannel(channel: SpeechChannel) -> OSStatus;
        fn SetSpeechProperty(
            channel: SpeechChannel,
            property: CFStringRef,
            value: CFTypeRef,
        ) -> OSStatus;
        fn SpeakCFString(channel: SpeechChannel, text: CFStringRef, options: CFTypeRef)
            -> OSStatus;
        fn StopSpeech(channel: SpeechChannel) -> OSStatus;
        fn SpeechBusy() -> i16;
        fn CountVoices(count: *mut i16) -> OSStatus;
        fn GetIndVoice(index: i16, voice: *mut VoiceSpec) -> OSStatus;
        fn GetVoiceDescription(
            voice: *const VoiceSpec,
            description: *mut VoiceDescription,
            length: i32,
        ) -> OSStatus;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFNumberCreate(allocator: CFTypeRef, kind: isize, value: *const c_void) -> CFNumberRef;
        fn CFStringCreateWithBytes(
            allocator: CFTypeRef,
            bytes: *const u8,
            length: isize,
            encoding: u32,
            external: u8,
        ) -> CFStringRef;
        fn CFRelease(value: CFTypeRef);
    }

    const WAVE: u32 = u32::from_be_bytes(*b"WAVE");
    const LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");
    const SIGNED_PACKED: u32 = 0x4 | 0x8;
    const NUMBER_SINT64: isize = 4;
    const NUMBER_FLOAT64: isize = 13;
    const UTF8: u32 = 0x0800_0100;

    #[derive(Default)]
    struct MemoryFile {
        bytes: Vec<u8>,
    }

    extern "C" fn read_callback(
        context: *mut c_void,
        position: i64,
        count: u32,
        output: *mut c_void,
        received: *mut u32,
    ) -> OSStatus {
        let memory = unsafe { &*(context as *const MemoryFile) };
        let position = position.max(0) as usize;
        let available = memory.bytes.len().saturating_sub(position);
        let copied = available.min(count as usize);
        if copied > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    memory.bytes.as_ptr().add(position),
                    output as *mut u8,
                    copied,
                );
            }
        }
        unsafe { *received = copied as u32 };
        0
    }

    extern "C" fn write_callback(
        context: *mut c_void,
        position: i64,
        count: u32,
        input: *const c_void,
        written: *mut u32,
    ) -> OSStatus {
        let memory = unsafe { &mut *(context as *mut MemoryFile) };
        let position = position.max(0) as usize;
        let Some(end) = position.checked_add(count as usize) else {
            return -1;
        };
        if memory.bytes.len() < end {
            memory.bytes.resize(end, 0);
        }
        if count > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    input as *const u8,
                    memory.bytes.as_mut_ptr().add(position),
                    count as usize,
                );
            }
        }
        unsafe { *written = count };
        0
    }

    extern "C" fn size_callback(context: *mut c_void) -> i64 {
        unsafe { (*(context as *const MemoryFile)).bytes.len() as i64 }
    }

    extern "C" fn set_size_callback(context: *mut c_void, size: i64) -> OSStatus {
        unsafe {
            (*(context as *mut MemoryFile))
                .bytes
                .resize(size.max(0) as usize, 0)
        };
        0
    }

    fn voice_named(name: &str) -> Option<VoiceSpec> {
        let mut count = 0_i16;
        if unsafe { CountVoices(&mut count) } != 0 {
            return None;
        }
        for index in 1..=count {
            let mut voice = VoiceSpec::default();
            if unsafe { GetIndVoice(index, &mut voice) } != 0 {
                continue;
            }
            let mut description: VoiceDescription = unsafe { std::mem::zeroed() };
            if unsafe {
                GetVoiceDescription(
                    &voice,
                    &mut description,
                    std::mem::size_of::<VoiceDescription>() as i32,
                )
            } != 0
            {
                continue;
            }
            let length = (description.name[0] as usize).min(63);
            let found = String::from_utf8_lossy(&description.name[1..1 + length]);
            if found.eq_ignore_ascii_case(name) {
                return Some(voice);
            }
        }
        None
    }

    pub fn speak(
        text: &str,
        voice: Option<&str>,
        words_per_minute: Option<u32>,
    ) -> VoiceResult<Vec<f32>> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let mut memory = Box::<MemoryFile>::default();
        let format = AudioDescription {
            sample_rate: mote_voice_core::SAMPLE_RATE as f64,
            format_id: LINEAR_PCM,
            format_flags: SIGNED_PACKED,
            bytes_per_packet: 2,
            frames_per_packet: 1,
            bytes_per_frame: 2,
            channels_per_frame: 1,
            bits_per_channel: 16,
            reserved: 0,
        };
        let client = &mut *memory as *mut MemoryFile as *mut c_void;
        let mut file = std::ptr::null_mut();
        let status = unsafe {
            AudioFileInitializeWithCallbacks(
                client,
                read_callback,
                write_callback,
                size_callback,
                set_size_callback,
                WAVE,
                &format,
                0,
                &mut file,
            )
        };
        if status != 0 {
            return Err(runtime(format!(
                "could not create in-memory speech output ({status})"
            )));
        }
        let mut extended = std::ptr::null_mut();
        let status = unsafe { ExtAudioFileWrapAudioFileID(file, 1, &mut extended) };
        if status != 0 {
            unsafe { AudioFileClose(file) };
            return Err(runtime(format!(
                "could not wrap in-memory speech output ({status})"
            )));
        }
        let selected_voice = voice.and_then(voice_named);
        let mut channel = std::ptr::null_mut();
        let status = unsafe {
            NewSpeechChannel(
                selected_voice
                    .as_ref()
                    .map_or(std::ptr::null(), |voice| voice as *const VoiceSpec),
                &mut channel,
            )
        };
        if status != 0 {
            close(std::ptr::null_mut(), extended, file);
            return Err(runtime(format!(
                "could not create speech channel ({status})"
            )));
        }
        let result = render(channel, extended, text, words_per_minute);
        close(channel, extended, file);
        result?;
        parse_wave(&memory.bytes)
    }

    fn render(
        channel: SpeechChannel,
        extended: ExtAudioFileRef,
        text: &str,
        words_per_minute: Option<u32>,
    ) -> VoiceResult<()> {
        let pointer = extended as i64;
        let number = unsafe {
            CFNumberCreate(
                std::ptr::null(),
                NUMBER_SINT64,
                &pointer as *const i64 as *const c_void,
            )
        };
        if number.is_null() {
            return Err(runtime("could not create speech output reference"));
        }
        let status =
            unsafe { SetSpeechProperty(channel, kSpeechOutputToExtAudioFileProperty, number) };
        unsafe { CFRelease(number) };
        if status != 0 {
            return Err(runtime(format!("could not set speech output ({status})")));
        }
        if let Some(words_per_minute) = words_per_minute {
            let rate = words_per_minute as f64;
            let number = unsafe {
                CFNumberCreate(
                    std::ptr::null(),
                    NUMBER_FLOAT64,
                    &rate as *const f64 as *const c_void,
                )
            };
            if !number.is_null() {
                unsafe {
                    SetSpeechProperty(channel, kSpeechRateProperty, number);
                    CFRelease(number);
                }
            }
        }
        let string = unsafe {
            CFStringCreateWithBytes(
                std::ptr::null(),
                text.as_ptr(),
                text.len() as isize,
                UTF8,
                0,
            )
        };
        if string.is_null() {
            return Err(runtime("could not create speech text"));
        }
        let status = unsafe { SpeakCFString(channel, string, std::ptr::null()) };
        unsafe { CFRelease(string) };
        if status != 0 {
            return Err(runtime(format!("could not speak line ({status})")));
        }
        let deadline = Duration::from_millis(20_000 + 100 * text.chars().count() as u64);
        let started = Instant::now();
        std::thread::sleep(Duration::from_millis(20));
        while unsafe { SpeechBusy() } > 0 {
            if started.elapsed() > deadline {
                unsafe { StopSpeech(channel) };
                return Err(runtime("speech synthesis timed out"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    fn close(channel: SpeechChannel, extended: ExtAudioFileRef, file: AudioFileId) {
        unsafe {
            if !channel.is_null() {
                DisposeSpeechChannel(channel);
            }
            ExtAudioFileDispose(extended);
            AudioFileClose(file);
        }
    }

    fn parse_wave(bytes: &[u8]) -> VoiceResult<Vec<f32>> {
        if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err(VoiceError::InvalidInput(
                "system speech returned invalid wave data".to_string(),
            ));
        }
        let mut position = 12;
        while position + 8 <= bytes.len() {
            let identifier = &bytes[position..position + 4];
            let size = u32::from_le_bytes(
                bytes[position + 4..position + 8]
                    .try_into()
                    .expect("four-byte wave size"),
            ) as usize;
            let start = position + 8;
            let Some(end) = start.checked_add(size) else {
                break;
            };
            if end > bytes.len() {
                break;
            }
            if identifier == b"data" {
                return Ok(mote_voice_core::pcm::from_s16le(&bytes[start..end]));
            }
            position = end + (size % 2);
        }
        Err(VoiceError::InvalidInput(
            "system speech wave contained no audio".to_string(),
        ))
    }

    fn runtime(message: impl Into<String>) -> VoiceError {
        VoiceError::Runtime(message.into())
    }
}
