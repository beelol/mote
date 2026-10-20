//! macOS voice-processing audio unit capture and playback.

use crate::{VoiceError, VoiceResult};
use mote_voice_core::{PcmFrame, FRAME_SAMPLES};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub type AudioCallback = Box<dyn FnMut(PcmFrame) + Send + 'static>;
pub type SharedOutput = Arc<Mutex<VecDeque<f32>>>;

pub trait Microphone: Send {
    fn start(&mut self, on_audio: AudioCallback) -> VoiceResult<()>;
    fn stop(&mut self) -> VoiceResult<()>;
    fn pause(&mut self) -> VoiceResult<()>;
    fn resume(&mut self) -> VoiceResult<()>;
}

#[derive(Default)]
pub struct MacMicrophone {
    voice_processing: bool,
    device: Option<platform::Device>,
    output: SharedOutput,
}

impl MacMicrophone {
    pub fn new(voice_processing: bool) -> Self {
        Self {
            voice_processing,
            device: None,
            output: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn output(&self) -> SharedOutput {
        Arc::clone(&self.output)
    }
}

impl Microphone for MacMicrophone {
    fn start(&mut self, mut on_audio: AudioCallback) -> VoiceResult<()> {
        if self.device.is_some() {
            return Ok(());
        }
        let mut pending = Vec::new();
        let device = platform::open(
            self.voice_processing,
            Arc::clone(&self.output),
            move |samples| {
                pending.extend_from_slice(&samples);
                let complete = pending.len() / FRAME_SAMPLES;
                if complete == 0 {
                    return;
                }
                let consumed = complete * FRAME_SAMPLES;
                for samples in pending[..consumed].chunks_exact(FRAME_SAMPLES) {
                    if let Ok(frame) = PcmFrame::try_from_slice(samples) {
                        on_audio(frame);
                    }
                }
                pending.drain(..consumed);
            },
        )?;
        self.device = Some(device);
        Ok(())
    }

    fn stop(&mut self) -> VoiceResult<()> {
        if let Some(device) = self.device.take() {
            device.stop()?;
        }
        Ok(())
    }

    fn pause(&mut self) -> VoiceResult<()> {
        match &mut self.device {
            Some(device) => device.pause(),
            None => Ok(()),
        }
    }

    fn resume(&mut self) -> VoiceResult<()> {
        match &mut self.device {
            Some(device) => device.resume(),
            None => Ok(()),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::{SharedOutput, VoiceError, VoiceResult};

    pub struct Device;

    impl Device {
        pub fn stop(self) -> VoiceResult<()> {
            Ok(())
        }

        pub fn pause(&self) -> VoiceResult<()> {
            Ok(())
        }

        pub fn resume(&self) -> VoiceResult<()> {
            Ok(())
        }
    }

    pub fn open<F>(
        _voice_processing: bool,
        _output: SharedOutput,
        _on_audio: F,
    ) -> VoiceResult<Device>
    where
        F: FnMut(Vec<f32>) + Send + 'static,
    {
        Err(VoiceError::Unavailable(
            "microphone capture is available on macOS only",
        ))
    }
}

#[cfg(target_os = "macos")]
mod platform {
    #![allow(non_snake_case, non_upper_case_globals)]

    use super::{SharedOutput, VoiceError, VoiceResult};
    use std::ffi::c_void;
    use std::sync::Arc;

    type OSStatus = i32;
    type AudioUnit = *mut c_void;

    #[repr(C)]
    struct AudioComponentDescription {
        componentType: u32,
        componentSubType: u32,
        componentManufacturer: u32,
        componentFlags: u32,
        componentFlagsMask: u32,
    }

    #[repr(C)]
    struct AudioStreamBasicDescription {
        mSampleRate: f64,
        mFormatID: u32,
        mFormatFlags: u32,
        mBytesPerPacket: u32,
        mFramesPerPacket: u32,
        mBytesPerFrame: u32,
        mChannelsPerFrame: u32,
        mBitsPerChannel: u32,
        mReserved: u32,
    }

    #[repr(C)]
    struct AudioBuffer {
        mNumberChannels: u32,
        mDataByteSize: u32,
        mData: *mut c_void,
    }

    #[repr(C)]
    struct AudioBufferList {
        mNumberBuffers: u32,
        mBuffers: [AudioBuffer; 1],
    }

    type RenderProc = extern "C" fn(
        *mut c_void,
        *mut u32,
        *const c_void,
        u32,
        u32,
        *mut AudioBufferList,
    ) -> OSStatus;

    #[repr(C)]
    struct AURenderCallbackStruct {
        inputProc: RenderProc,
        inputProcRefCon: *mut c_void,
    }

    #[link(name = "AudioToolbox", kind = "framework")]
    extern "C" {
        fn AudioComponentFindNext(
            component: *mut c_void,
            description: *const AudioComponentDescription,
        ) -> *mut c_void;
        fn AudioComponentInstanceNew(component: *mut c_void, instance: *mut AudioUnit) -> OSStatus;
        fn AudioComponentInstanceDispose(instance: AudioUnit) -> OSStatus;
        fn AudioUnitSetProperty(
            unit: AudioUnit,
            identifier: u32,
            scope: u32,
            element: u32,
            data: *const c_void,
            data_size: u32,
        ) -> OSStatus;
        fn AudioUnitInitialize(unit: AudioUnit) -> OSStatus;
        fn AudioUnitUninitialize(unit: AudioUnit) -> OSStatus;
        fn AudioOutputUnitStart(unit: AudioUnit) -> OSStatus;
        fn AudioOutputUnitStop(unit: AudioUnit) -> OSStatus;
        fn AudioUnitRender(
            unit: AudioUnit,
            flags: *mut u32,
            timestamp: *const c_void,
            bus: u32,
            frames: u32,
            data: *mut AudioBufferList,
        ) -> OSStatus;
    }

    const AUDIO_UNIT_OUTPUT: u32 = u32::from_be_bytes(*b"auou");
    const VOICE_PROCESSING_IO: u32 = u32::from_be_bytes(*b"vpio");
    const APPLE: u32 = u32::from_be_bytes(*b"appl");
    const LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");
    const FLOAT_PACKED: u32 = 1 | 8;
    const ENABLE_IO: u32 = 2003;
    const SET_INPUT_CALLBACK: u32 = 2005;
    const STREAM_FORMAT: u32 = 8;
    const SET_RENDER_CALLBACK: u32 = 23;
    const BYPASS_VOICE_PROCESSING: u32 = 2100;
    const GLOBAL: u32 = 0;
    const INPUT: u32 = 1;
    const OUTPUT: u32 = 2;
    const INPUT_BUS: u32 = 1;
    const OUTPUT_BUS: u32 = 0;

    struct Context {
        unit: AudioUnit,
        on_audio: Box<dyn FnMut(Vec<f32>) + Send>,
        output: SharedOutput,
        buffer: Vec<f32>,
    }

    extern "C" fn recorded(
        context: *mut c_void,
        flags: *mut u32,
        timestamp: *const c_void,
        _bus: u32,
        frames: u32,
        _data: *mut AudioBufferList,
    ) -> OSStatus {
        let context = unsafe { &mut *(context as *mut Context) };
        if context.buffer.len() < frames as usize {
            context.buffer.resize(frames as usize, 0.0);
        }
        let mut list = AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [AudioBuffer {
                mNumberChannels: 1,
                mDataByteSize: frames * 4,
                mData: context.buffer.as_mut_ptr() as *mut c_void,
            }],
        };
        let status = unsafe {
            AudioUnitRender(context.unit, flags, timestamp, INPUT_BUS, frames, &mut list)
        };
        if status == 0 {
            (context.on_audio)(context.buffer[..frames as usize].to_vec());
        }
        status
    }

    extern "C" fn play(
        context: *mut c_void,
        _flags: *mut u32,
        _timestamp: *const c_void,
        _bus: u32,
        frames: u32,
        data: *mut AudioBufferList,
    ) -> OSStatus {
        let context = unsafe { &mut *(context as *mut Context) };
        let list = unsafe { &mut *data };
        let output = unsafe {
            std::slice::from_raw_parts_mut(list.mBuffers[0].mData as *mut f32, frames as usize)
        };
        let Ok(mut queue) = context.output.try_lock() else {
            output.fill(0.0);
            return 0;
        };
        for sample in output {
            *sample = queue.pop_front().unwrap_or_default();
        }
        0
    }

    pub struct Device {
        unit: AudioUnit,
        context: *mut Context,
        initialized: bool,
        running: bool,
        disposed: bool,
    }

    unsafe impl Send for Device {}

    impl Device {
        pub fn stop(mut self) -> VoiceResult<()> {
            self.shutdown()
        }

        pub fn pause(&mut self) -> VoiceResult<()> {
            if !self.running {
                return Ok(());
            }
            check(unsafe { AudioOutputUnitStop(self.unit) }, "pausing audio")?;
            self.running = false;
            Ok(())
        }

        pub fn resume(&mut self) -> VoiceResult<()> {
            if self.running {
                return Ok(());
            }
            check(unsafe { AudioOutputUnitStart(self.unit) }, "resuming audio")?;
            self.running = true;
            Ok(())
        }

        fn shutdown(&mut self) -> VoiceResult<()> {
            if self.disposed {
                return Ok(());
            }
            let stop_status = if self.running {
                unsafe { AudioOutputUnitStop(self.unit) }
            } else {
                0
            };
            unsafe {
                if self.initialized {
                    AudioUnitUninitialize(self.unit);
                }
                AudioComponentInstanceDispose(self.unit);
                if !self.context.is_null() {
                    drop(Box::from_raw(self.context));
                }
            }
            self.disposed = true;
            check(stop_status, "stopping audio")
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            let _ = self.shutdown();
        }
    }

    fn check(status: OSStatus, action: &str) -> VoiceResult<()> {
        if status == 0 {
            Ok(())
        } else {
            Err(VoiceError::Runtime(format!(
                "{action} failed with OSStatus {status}"
            )))
        }
    }

    pub fn open<F>(voice_processing: bool, output: SharedOutput, on_audio: F) -> VoiceResult<Device>
    where
        F: FnMut(Vec<f32>) + Send + 'static,
    {
        unsafe {
            let description = AudioComponentDescription {
                componentType: AUDIO_UNIT_OUTPUT,
                componentSubType: VOICE_PROCESSING_IO,
                componentManufacturer: APPLE,
                componentFlags: 0,
                componentFlagsMask: 0,
            };
            let component = AudioComponentFindNext(std::ptr::null_mut(), &description);
            if component.is_null() {
                return Err(VoiceError::Unavailable(
                    "macOS has no voice-processing audio unit",
                ));
            }
            let mut unit = std::ptr::null_mut();
            check(
                AudioComponentInstanceNew(component, &mut unit),
                "opening audio",
            )?;
            let mut device = Device {
                unit,
                context: std::ptr::null_mut(),
                initialized: false,
                running: false,
                disposed: false,
            };
            let one = 1_u32;
            check(
                AudioUnitSetProperty(
                    unit,
                    ENABLE_IO,
                    INPUT,
                    INPUT_BUS,
                    &one as *const u32 as *const c_void,
                    4,
                ),
                "enabling microphone",
            )?;
            check(
                AudioUnitSetProperty(
                    unit,
                    ENABLE_IO,
                    OUTPUT,
                    OUTPUT_BUS,
                    &one as *const u32 as *const c_void,
                    4,
                ),
                "enabling speakers",
            )?;
            let bypass = u32::from(!voice_processing);
            let _ = AudioUnitSetProperty(
                unit,
                BYPASS_VOICE_PROCESSING,
                GLOBAL,
                INPUT_BUS,
                &bypass as *const u32 as *const c_void,
                4,
            );
            let format = AudioStreamBasicDescription {
                mSampleRate: mote_voice_core::SAMPLE_RATE as f64,
                mFormatID: LINEAR_PCM,
                mFormatFlags: FLOAT_PACKED,
                mBytesPerPacket: 4,
                mFramesPerPacket: 1,
                mBytesPerFrame: 4,
                mChannelsPerFrame: 1,
                mBitsPerChannel: 32,
                mReserved: 0,
            };
            let format_size = std::mem::size_of::<AudioStreamBasicDescription>() as u32;
            check(
                AudioUnitSetProperty(
                    unit,
                    STREAM_FORMAT,
                    OUTPUT,
                    INPUT_BUS,
                    &format as *const _ as *const c_void,
                    format_size,
                ),
                "setting microphone format",
            )?;
            check(
                AudioUnitSetProperty(
                    unit,
                    STREAM_FORMAT,
                    INPUT,
                    OUTPUT_BUS,
                    &format as *const _ as *const c_void,
                    format_size,
                ),
                "setting speaker format",
            )?;
            let context = Box::into_raw(Box::new(Context {
                unit,
                on_audio: Box::new(on_audio),
                output: Arc::clone(&output),
                buffer: vec![0.0; 4096],
            }));
            device.context = context;
            let input_callback = AURenderCallbackStruct {
                inputProc: recorded,
                inputProcRefCon: context as *mut c_void,
            };
            check(
                AudioUnitSetProperty(
                    unit,
                    SET_INPUT_CALLBACK,
                    GLOBAL,
                    INPUT_BUS,
                    &input_callback as *const _ as *const c_void,
                    std::mem::size_of::<AURenderCallbackStruct>() as u32,
                ),
                "installing microphone callback",
            )?;
            let output_callback = AURenderCallbackStruct {
                inputProc: play,
                inputProcRefCon: context as *mut c_void,
            };
            check(
                AudioUnitSetProperty(
                    unit,
                    SET_RENDER_CALLBACK,
                    INPUT,
                    OUTPUT_BUS,
                    &output_callback as *const _ as *const c_void,
                    std::mem::size_of::<AURenderCallbackStruct>() as u32,
                ),
                "installing speaker callback",
            )?;
            check(AudioUnitInitialize(unit), "initializing audio")?;
            device.initialized = true;
            check(AudioOutputUnitStart(unit), "starting audio")?;
            device.running = true;
            Ok(device)
        }
    }
}
