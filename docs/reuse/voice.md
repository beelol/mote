# Reuse: voice

Concept 14 in `docs/concepts.md`. Deferred behind DEF-03 in the README. Feeds the attention
detector (`attention.md`) and the talk lane (`talk.md`). Written 2026-10-08. Every number has a
source in the last section. Numbers from other boards need a run on the owner's hardware before
they count as evidence.

## Key points

1. Speech to text is Moonshine v2 through the official `sherpa-onnx` crate on both machines. On a Pi 5 it reaches a final transcript 237 ms (Tiny), 527 ms (Small) or 802 ms (Medium) after speech ends. Whisper Tiny takes 5.9 s and Whisper Small 10.4 s on the same board. Code and streaming weights are MIT.
2. Moonshine Medium beats whisper.cpp small.en on accuracy too: 2.08% against 4.28% word error rate on LibriSpeech clean, and 107 ms against 122 ms after speech end on a Mac. whisper.cpp stays the alternative for languages Moonshine lacks. Parakeet v3 is the accuracy ceiling at 1.93%.
3. Text to speech is Kyutai Pocket TTS, 100M parameters, MIT code and CC-BY-4.0 weights, streaming, about 200 ms to the first chunk and 6x real time on two M4 cores. Piper is the Pi fallback with a measured real-time factor of 0.36 on a Pi 4, but its engine is GPL-3.0 and runs as a separate process.
4. Kokoro sounds better than Piper but is not real time on a Pi 4 (real-time factor 2.8 to 3.2 on four threads). It stays a Mac option.
5. Echo cancellation is `sonora`, a pure-Rust port of WebRTC AEC3 that costs 4.2 us per 10 ms frame on an M4 Max. The macOS voice processing unit and PipeWire's echo-cancel module are the system-level alternatives.
6. v0 is half duplex with barge-in in the Pipecat shape: an interrupt event cancels the model stream, flushes the audio queue, and keeps only the words that were played in the context.
7. Full duplex in the Moshi style needs a 7B model and Metal. It fits the Mac and never the Pi.
8. The budget is speech end to first audio under 1 s on the Mac and under 2.5 s on the Pi, where the local talk model, not speech, is the slow stage.
9. Nothing leaves the device. `sherpa-rs` is deprecated upstream in favour of the official crate. Python projects are sidecar only and cost 300 MB to 1 GB of RSS on a Pi.

## What the concept needs

- Capture: 16 kHz mono frames from the default input on macOS and ALSA or PipeWire on the Pi.
- Playback with cancel: stop within one buffer on barge-in.
- Echo cancellation so the mic does not hear the speaker.
- STT: under 300 ms from end of speech to final text on the Mac, under 1 s on the Pi.
- TTS: first audio within 300 ms, sentence by sentence.
- Endpointing: end of speech decided within 300 to 500 ms.
- A pipeline: capture, VAD, STT, attention, talk, TTS, playback, with a barge-in path back to capture.
- Ledger rows for every STT and TTS call: model, audio seconds, wall time, real-time factor.
- The DEF-03 approval before any of it is built.

## Candidates

### Speech to text

| Name | Repo | Language | License | Last release | Use | Reason | Pin |
|---|---|---|---|---|---|---|---|
| Moonshine v2 streaming | https://github.com/moonshine-ai/moonshine | C++ core, ONNX | MIT code and streaming weights, older non-English models non-commercial | 2026-02-13 | model through sherpa-onnx (default) | Tiny 34M, Small 123M, Medium 245M. Pi 5: 237 / 527 / 802 ms after speech end. Mac: 34 / 73 / 107 ms. WER clean 4.49 / 2.49 / 2.08 in float, 4.83 / 2.61 / 2.17 for the shipped 8-bit files | model file hash from the sherpa-onnx release |
| sherpa-onnx (official Rust crate) | https://github.com/k2-fsa/sherpa-onnx | Rust over C++ | Apache-2.0 | 1.13.8, 2026-09-11 | embed as crate | One runtime for Moonshine, Parakeet, Zipformer streaming, Whisper, Pocket TTS, Piper voices, Kokoro, Matcha, Silero VAD and speaker embeddings | `1.13.8` |
| whisper.cpp | https://github.com/ggml-org/whisper.cpp | C++ | MIT | v1.9.5, 2026-10-06 | keep as alternative | Metal, 99 languages, small.en 122 ms after speech end and WER 4.28% on an M5 Max, chunked not streaming, Pi 5 numbers only as proxies (tiny about 2 to 3 s per 10 s clip) | `whisper-rs` 0.16.0 |
| whisper-rs | https://codeberg.org/tazz4843/whisper-rs | Rust | Unlicense | 0.16.0, 2026-03-12 | embed as crate (alternative) | Maintained bindings with a `metal` feature, no `coreml` feature listed | `0.16.0` |
| Parakeet TDT 0.6B v3 | https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3 | ONNX through sherpa-onnx | CC-BY-4.0 | 2025-08-14 | keep as accuracy alternative | WER clean 1.93%, 25 languages, int8 640 MB, real-time factor 0.088 on four Cortex-A76 cores (the Pi 5 core), offline decode only | model file hash |
| Kyutai STT 1B | https://github.com/kyutai-labs/delayed-streams-modeling | Rust and Python | Apache-2.0 Rust, CC-BY-4.0 weights | `moshi-server` 0.6.4, 2025-10-01 | port the idea | True streaming with a 0.5 s delay and a semantic end-of-turn signal, Metal only, no CPU path documented | `0.6.4` |
| sherpa-onnx streaming Zipformer | https://k2-fsa.github.io/sherpa/onnx/pretrained_models/online-transducer/zipformer-transducer-models.html | ONNX | Apache-2.0 | rolling | keep as alternative | True streaming, en-20M int8 at real-time factor 0.038, accuracy unpublished | model file hash |
| faster-whisper | https://github.com/SYSTRAN/faster-whisper | Python | MIT | 1.2.1, 2025-10-31 | sidecar only | No Metal, Pi 5 tiny 2 to 3 s per 10 s clip, Python runtime | skip |
| Vosk | https://github.com/alphacep/vosk-api | C++ (Kaldi) | Apache-2.0 | 0.3.50 | skip | Streaming and small, accuracy unpublished and below the models above | skip |
| Apple SpeechAnalyzer | macOS 26 framework | Swift | Apple | macOS 26 | skip for v0 | WER 1.98% and 125 ms on an M5 Max, Mac only, Rust access through objc2 is unverified | skip |
| candle whisper example | https://github.com/huggingface/candle | Rust | MIT OR Apache-2.0 | `candle-core` 0.11.0 | skip | File-based example, no streaming, no published speed | skip |

### Text to speech

| Name | Repo | Language | License | Last release | Use | Reason | Pin |
|---|---|---|---|---|---|---|---|
| Kyutai Pocket TTS | https://github.com/kyutai-labs/pocket-tts | Python, Rust ports | MIT code, CC-BY-4.0 weights | 2026-01-13 | model through sherpa-onnx (default) | 100M, six languages, voice from a WAV, 200 ms first chunk and 6x real time on two M4 Air cores, Pi 5 unknown | model file hash |
| pocket-tts (Rust) | https://github.com/babybirdprd/pocket-tts | Rust (candle) | MIT OR Apache-2.0 | 0.6.2, 2026-02-13 | keep as alternative | Pure Rust path if sherpa-onnx's Pocket support misses | `0.6.2` |
| Piper | https://github.com/OHF-Voice/piper1-gpl | C++ and Python | GPL-3.0 engine, voices per voice card | v1.8.0, 2026-09-04 | sidecar (Pi fallback) | Real-time factor 0.36 on four Pi 4 threads, under 1 s per sentence on a Pi 5, GPL engine stays out of the daemon's process | binary release hash |
| piper-rs | https://github.com/thewh1teagle/piper-rs | Rust | MIT, links GPL espeak-ng | 0.2.0, 2026-05-21 | skip | Linking espeak-ng pulls GPL-3.0 into the MIT daemon | skip |
| Kokoro 82M | https://github.com/hexgrad/kokoro | ONNX through sherpa-onnx | Apache-2.0 | v1.0, 2025-01-27 | keep as Mac option | Best voice quality in this list, real-time factor 2.8 to 3.2 on four Pi 4 threads, needs espeak-ng for out-of-vocabulary words | model file hash |
| Matcha TTS | https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/matcha.html | ONNX through sherpa-onnx | Apache-2.0 | rolling | keep as Pi alternative | Real-time factor 0.41 on four Pi 4 threads, 71 MB plus a 51 MB vocoder | model file hash |
| Kitten TTS nano / mini | https://github.com/KittenML/KittenTTS | ONNX through sherpa-onnx | Apache-2.0 | 2025 | measure on the Pi | 15M to 80M, 20 languages, no published real-time factor | model file hash |
| Kyutai TTS 1.6B | https://github.com/kyutai-labs/delayed-streams-modeling | Rust | Apache-2.0 Rust, CC-BY-4.0 weights | 0.6.4 | port the idea | 220 ms first audio on a GPU, Metal on the Mac, no CPU path, not a Pi option | `0.6.4` |
| Orpheus 3B | https://github.com/canopyai/Orpheus-TTS | Python, GGUF | Apache-2.0 | 2025-05 | skip | Needs a sustained 82 tokens per second plus a SNAC decoder, Mac speed unknown, 37x slower than real time on a 12-core CPU | skip |
| Chatterbox, Dia, Fish Speech, F5-TTS, VibeVoice | various | Python | MIT, Apache-2.0, non-commercial | 2025 | skip | GPU-class models or non-commercial weights | skip |
| Apple AVSpeechSynthesizer | macOS framework | Swift | Apple | macOS | skip for v0 | Mac only | skip |

### Duplex, capture, echo cancellation

| Name | Repo | Language | License | Last release | Use | Reason | Pin |
|---|---|---|---|---|---|---|---|
| Moshi | https://github.com/kyutai-labs/moshi | Rust and Python | Apache-2.0 Rust, CC-BY-4.0 weights | `moshi` 0.6.4, 2025-10-01 | port the idea | Full duplex, 7B plus the Mimi codec at 12.5 Hz, 160 ms theoretical latency, `moshi-backend --features metal` with an int8 config, no Pi path | `0.6.4` |
| cpal | https://github.com/RustAudio/cpal | Rust | Apache-2.0 | 0.18.2, 2026-08-16 | embed as crate | CoreAudio on macOS 14.2 and up, ALSA by default on Linux with `pipewire` and `jack` features, no voice processing path | `0.18.2` |
| rodio | https://github.com/RustAudio/rodio | Rust | MIT OR Apache-2.0 | 0.22.2, 2026-03-05 | embed as crate | Playback with a stoppable sink over cpal | `0.22.2` |
| sonora | https://github.com/dignifiedquire/sonora | Rust | BSD-3-Clause | 0.2.0, 2026-07-29 | embed as crate (default AEC) | Pure-Rust port of WebRTC M145 audio processing with AEC3, noise suppression and gain control, passes the 2400-test upstream suite, 4.2 us per 10 ms frame on an M4 Max, NEON on macOS arm64, scalar fallback elsewhere | `0.2.0` |
| webrtc-audio-processing | https://github.com/tonarino/webrtc-audio-processing | Rust over C++ | BSD-3-Clause | 2.1.0, 2026-05-13 | keep as alternative | The mature wrapper, needs meson and ninja for the bundled build, aarch64 packages exist | `2.1.0` |
| coreaudio-rs | https://github.com/RustAudio/coreaudio-rs | Rust | MIT OR Apache-2.0 | 0.14.2, 2026-04-29 | keep as Mac alternative | Opens `kAudioUnitSubType_VoiceProcessingIO` for the system AEC, with mono and shared input-output constraints | `0.14.2` |
| PipeWire echo-cancel module | https://docs.pipewire.org/page_module_echo_cancel.html | C | MIT | Pi OS Bookworm | keep as Pi alternative | System-level WebRTC AEC with no code in the daemon | n/a |
| ReSpeaker XVF3800 USB array | https://wiki.seeedstudio.com/respeaker_xvf3800_introduction/ | hardware | n/a | 2024 | hardware option (Pi) | On-chip AEC, beamforming and noise suppression. The 2-mic HAT has no AEC | n/a |
| skadoosh | https://docs.rs/crate/skadoosh/latest | Rust | MIT OR Apache-2.0 | 0.12.1, 2026-08-23 | port the idea | A Rust Silero, whisper-rs, Ollama and Kokoro pipeline with barge-in in 5 to 10 ms, pins the same `ort` as the router | reference only |
| Pipecat, LiveKit Agents | https://github.com/pipecat-ai/pipecat, https://github.com/livekit/agents | Python | BSD-2-Clause, Apache-2.0 | 2026 | port the idea | Interruption frames, context trimmed to played words, 0.5 s minimum interruption, endpointing between 0.5 and 3 s | reference only |

## Recommendation

```toml
# crates/mote-voice/Cargo.toml
[dependencies]
cpal = { git = "https://github.com/RustAudio/cpal", tag = "v0.18.2" }
rodio = { git = "https://github.com/RustAudio/rodio", tag = "v0.22.2" }
sonora = { git = "https://github.com/dignifiedquire/sonora", tag = "v0.2.0" }
sherpa-onnx = { git = "https://github.com/k2-fsa/sherpa-onnx", tag = "v1.13.8" }
earshot = { git = "https://github.com/pykeio/earshot", tag = "v1.2.2" }

[target.'cfg(target_os = "macos")'.dependencies]
whisper-rs = { git = "https://codeberg.org/tazz4843/whisper-rs", tag = "v0.16.0", features = ["metal"], optional = true }
```

Replace each `tag` with the exact `rev` after `git ls-remote --tags` confirms the tag name. Model files are fetched once by `moted voice fetch` with the owner's approval and checked by hash: Moonshine v2 Medium and Pocket TTS on the Mac, Moonshine v2 Tiny or Small and Pocket TTS on the Pi, with a Piper voice as the Pi fallback.

Piper on the Pi runs as its own process so the GPL-3.0 engine never links into the daemon:

```sh
piper --model en_US-lessac-medium.onnx --output-raw < sentence.txt | aplay -r 22050 -f S16_LE
```

Pipeline shape: one capture thread writes 16 ms frames through `sonora` into a ring. `earshot` gates frames, Silero decides utterance boundaries, and each utterance goes to Moonshine. Final text goes to the attention detector, then to the talk lane. Pocket TTS runs per sentence and streams to a stoppable sink. During playback the VAD keeps reading the echo-cancelled mic. Voice onset longer than 500 ms, the LiveKit default, stops the sink, cancels the talk stream, and marks the reply as interrupted at the last played word.

## Efficiency targets and what to measure

| Stage | Mac | Pi 5 | How measured |
|---|---|---|---|
| End of speech to final text | < 150 ms | < 600 ms | event log timestamps, Moonshine Medium on the Mac, Small on the Pi |
| Text to first audio | < 250 ms | < 800 ms | timestamps |
| Speech end to first audio, end to end | < 1000 ms | < 2500 ms | `moted bench voice` on 20 fixture turns |
| STT real-time factor | < 0.1 | < 0.3 | bench |
| TTS real-time factor | < 0.2 | < 0.8 | bench |
| Barge-in stop | < 100 ms | < 200 ms | timestamps |
| AEC cost | < 20 us per 10 ms frame | < 200 us | bench |
| Resident RAM for voice | < 1 GiB | < 700 MiB | process RSS |
| Residual echo | VAD silent during playback of a fixture | same | VAD events during playback |

Published anchors: a voice agent budget of about 680 ms p50 across capture, STT, model and TTS (ElevenLabs), and 800 ms as the point where a pause feels unnatural (Daily). On a Pi 5 the speech stages fit in about 1 s. The talk model's TTFT is the remaining cost, measured in `talk.md`.

## Open decisions with defaults

1. STT. Default: Moonshine v2 through `sherpa-onnx`, Medium on the Mac, Small on the Pi. whisper.cpp small.en through `whisper-rs` when a non-English language is needed. Parakeet v3 when accuracy on the fixture set matters more than latency.
2. Streaming decode. Default: sherpa-onnx's offline Moonshine per utterance. Moonshine's own C++ core through FFI only when the measured latency after speech end misses the target, since the published 237 ms uses its streaming encoder.
3. TTS. Default: Pocket TTS through `sherpa-onnx` on both machines. Piper as a separate process on the Pi when Pocket's real-time factor is above 0.8. Kokoro on the Mac when voice quality is the complaint.
4. Phonemizer license. Default: nothing that links espeak-ng (GPL-3.0) enters the daemon. Verify whether sherpa-onnx's Pocket and Matcha paths bundle it before pinning.
5. Echo cancellation. Default: `sonora` in-process on both. The macOS voice processing unit through `coreaudio-rs`, or PipeWire's module on the Pi, when residual echo stays audible. A ReSpeaker XVF3800 when software AEC fails.
6. Duplex. Default: half duplex with barge-in. Moshi on the Mac is a separate approval.
7. Endpointing. Default: Silero silence of 500 ms, cut to 300 ms when the partial text ends a sentence. Kyutai's semantic end-of-turn is the Mac upgrade.
8. Capture. Default: `cpal`, 16 kHz mono, 16 ms frames, a fixed ALSA buffer size on the Pi.
9. Languages. Default: English only in v0.
10. AI HAT+ 2. Default: not planned. It lists speech to text generically and has no published latency.

## Acceptance criteria

- **VOICE-01** `cpal` capture delivers 16 kHz mono 16 ms frames on macOS and on the Pi's Linux. **Verify:** `cargo test -p mote-voice capture::` with a loopback fixture plus a live run on each machine.
- **VOICE-02** STT transcribes the fixture set with a word error rate at most 2 points above the chosen model's published WER. **Verify:** `moted bench voice --stt` on the fixture set.
- **VOICE-03** TTS first audio arrives within the target for a 20-word sentence. **Verify:** `moted bench voice --tts`.
- **VOICE-04** Speech during playback stops playback within the target, cancels the talk stream, and the event log marks the reply as interrupted at the last played word. **Verify:** scripted barge-in test.
- **VOICE-05** Playback of a fixture through the speaker does not trigger VAD on the mic. **Verify:** VAD event count during playback is zero on each machine.
- **VOICE-06** End of speech is detected within the target on fixtures with silence tails. **Verify:** `cargo test -p mote-voice endpoint::`.
- **VOICE-07** Every STT and TTS call writes a ledger row with model, audio seconds, wall time and real-time factor. **Verify:** `SELECT` after a live turn.
- **VOICE-08** The voice process opens no outbound socket. **Verify:** `lsof -p` during a live session.
- **VOICE-09** The Pi profile picks the Pi models under 16 GiB total memory. **Verify:** memory fixture test.
- **VOICE-10** Speech end to first audio p50 is within the target on each machine. **Verify:** `moted bench voice` output in the evidence file.
- **VOICE-11** No GPL-3.0 code links into `moted`. **Verify:** `cargo tree -e normal` shows no espeak-ng or Piper crate, and the Piper fallback runs as a child process.

## Sources

- Moonshine v2: https://github.com/moonshine-ai/moonshine, https://arxiv.org/abs/2602.12241, https://moonshine-voice.readthedocs.io/en/latest/models/accuracy/, https://mintlify.wiki/moonshine-ai/moonshine/models/benchmarks
- sherpa-onnx crate and docs: https://crates.io/crates/sherpa-onnx, https://k2-fsa.github.io/sherpa/onnx/moonshine/index.html, https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/index.html
- whisper.cpp: https://github.com/ggml-org/whisper.cpp/releases and the M5 Max comparison https://dev.to/iravoice/apple-speechanalyzer-vs-whispercpp-a-40-speaker-mac-benchmark-40i4
- whisper-rs: https://crates.io/crates/whisper-rs
- Parakeet v3: https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3 and Cortex-A76 timings https://k2-fsa.github.io/sherpa/onnx/pretrained_models/offline-transducer/nemo-transducer-models.html
- Kyutai STT, TTS and Pocket TTS: https://github.com/kyutai-labs/delayed-streams-modeling, https://kyutai.org/stt, https://kyutai.org/next/tts, https://github.com/kyutai-labs/pocket-tts
- pocket-tts crate: https://crates.io/crates/pocket-tts
- Piper: https://github.com/OHF-Voice/piper1-gpl/releases and Pi numbers https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/vits.html
- piper-rs: https://crates.io/crates/piper-rs
- Kokoro and its Pi 4 numbers: https://huggingface.co/hexgrad/Kokoro-82M, https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/kokoro.html
- Matcha: https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/matcha.html
- Kitten TTS: https://github.com/KittenML/KittenTTS
- Orpheus CPU numbers: https://github.com/canopyai/Orpheus-TTS/blob/main/additional_inference_options/no_gpu/README.md
- Moshi: https://github.com/kyutai-labs/moshi/blob/main/rust/README.md
- cpal: https://crates.io/crates/cpal and https://github.com/RustAudio/cpal
- rodio: https://crates.io/crates/rodio
- sonora: https://github.com/dignifiedquire/sonora
- webrtc-audio-processing: https://crates.io/crates/webrtc-audio-processing
- coreaudio-rs and the voice processing unit: https://crates.io/crates/coreaudio-rs, https://developer.apple.com/forums/thread/66953
- PipeWire echo cancel: https://docs.pipewire.org/page_module_echo_cancel.html
- ReSpeaker: https://wiki.seeedstudio.com/respeaker_xvf3800_introduction/ and https://wiki.seeedstudio.com/ReSpeaker_2_Mics_Pi_HAT_Raspberry/
- skadoosh: https://docs.rs/crate/skadoosh/latest
- Pipecat interruptions: https://docs.pipecat.ai/pipecat/fundamentals/interruptions
- LiveKit turn tuning: https://docs.livekit.io/agents/logic/turns/tuning/
- Latency budgets: https://www.daily.co/blog/the-worlds-fastest-voice-bot/ and https://elevenlabs.io/blog/voice-agent-latency-optimization
- Pi 5 end-to-end write-ups: https://www.fast.io/resources/openclaw-raspberry-pi-speech-recognition-offline-voice-agent.md, https://raspberry.tips/en/smart-home/local-voice-assistant-replace-alexa-with-home-assistant-and-raspberry-pi-5
- AI HAT+ 2: https://www.raspberrypi.com/products/ai-hat-plus-2/
