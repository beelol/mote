# Reuse: attention detector

Concept 2 in `docs/concepts.md`. Serves MOTE-22, MOTE-23 and MOTE-24 in `docs/rfcs/mote.md`.
Written 2026-10-08. Numbers marked "approx." are estimates from published figures on other
hardware and need a run on the owner's machines before they count as evidence.

## Key points

1. No open model or dataset answers "is the owner talking to the device". The one commercial product, attenlabs SAS, runs in the cloud. mote ports the idea from the Amazon, Apple and Microsoft papers: fuse an acoustic signal with the transcript in one small scorer.
2. The published pattern since 2023 is ASR text plus a small audio signal into one language model. Amazon's 2018 fusion cut EER from 9.3% to 5.2%. Apple's 2024 multimodal model cut EER by 39% against text alone.
3. v0 pipeline: Silero VAD cuts utterances, a CAM++ speaker embedding says whether the voice is the owner, the STT lane fills a rolling transcript, and `qwen3:0.6b` scores the window as addressed or not.
4. The scorer returns `{"addressed": bool, "sureness": 0 to 100, "reason": "..."}` through Ollama structured output with thinking off. Anything malformed scores 0 and stays silent.
5. Calibration uses token probabilities, not a verbalized number. Ollama's native API has `logprobs` since 0.12.11. The score is `logit P(yes) - logit P(no)` at the first answer token, fitted with temperature scaling on labelled corrections.
6. Everything embeds in Rust through the official `sherpa-onnx` crate and `mote-localmodel`. No Python sidecar.
7. Three thresholds are settings. Defaults: answer at 75 and above, ask from 40 to 75, silent below 40. The owner's described band of 40 to 50 is where calibration starts.
8. On the Pi 5 the 0.6B scorer costs about 3.5 s per 200-token prompt through Ollama, so the Pi keeps the prompt under 60 tokens with a cached prefix, and switches to the router's trained text head once labels exist.
9. Nothing leaves the device. The detector opens no sockets, stores no raw audio, calls no tools and writes no memory. Speaker verification is biometric data, so guests need to be told.

## What the concept needs

- 16 kHz mono audio frames from the capture layer in `voice.md`.
- VAD that cuts utterances with start and end timestamps and an end-of-speech decision under 300 ms.
- Speaker verification: one enrolled owner embedding, a cosine score per utterance.
- A rolling transcript window from the STT lane: the last N utterances with speaker tag and time.
- A text scorer with one JSON verdict and token probabilities, parsed fail-closed.
- A fusion rule from speaker score and text score to one sureness from 0 to 100.
- Three thresholds as settings and three actions: answer through the talk lane, ask one short question, stay silent.
- A labelled fixture set: addressed, not addressed, ambiguous.
- Verdict rows in the event log with the reason and the input ids, never raw audio.
- No tools and no memory writes (MOTE-22).

## Candidates

### Research to port

| Paper | Org, year | Signals | Result | URL |
|---|---|---|---|---|
| Learning when to listen | Microsoft and SRI, 2012 | Lexical features plus energy-contour prosody | With real ASR: lexical EER 23.1%, prosody 12.6%, combined 11.1% | https://www.isca-archive.org/interspeech_2012/shriberg12_interspeech.html |
| Device-directed utterance detection | Amazon, 2018 | Acoustic LSTM, ASR 1-best LSTM, decoder graph features | EER: decoder 9.3%, acoustic 10.9%, text 20.1%, fused 5.2% | https://arxiv.org/abs/1808.02504 |
| Acoustic-only attention classifier | Nuance and McGill, 2019 | Acoustic CNN and RNN with attention | EER 15.6% to 16.3% on two sets | https://arxiv.org/abs/1902.00570 |
| Semantic lexical features for device directedness | Amazon, 2020 | Word embeddings of current and previous utterance plus ASR confidence | EER 9.1% against 10.6% acoustic only | https://arxiv.org/abs/2010.01949 |
| Streaming on-device detection from voice and touch invocation | Apple, 2022 | Streaming temporal CNN over acoustics | Matches non-streaming at 33% less peak memory | https://machinelearning.apple.com/research/streaming-on-device |
| Leveraging LLMs for exploiting ASR uncertainty | Apple, 2023 | ASR n-best list into a LoRA-tuned LLM | n-best beats 1-best | https://arxiv.org/abs/2309.04842 |
| Multimodal device-directed speech detection with LLMs | Apple, 2024 | Audio encoder prefix, ASR 1-best and decoder confidence into one frozen LLM | EER down 39% against text only and 61% against audio only | https://arxiv.org/abs/2403.14438 |
| Device-directed detection for follow-up conversations | Apple, 2024 | First query plus follow-up text and ASR uncertainty in the prompt | 20% to 40% fewer false alarms at 10% false reject | https://arxiv.org/abs/2411.00023 |
| Look and Talk | Google, 2022 | Gaze, distance, face match, then voice match, on-device ASR, a text intent model and a non-lexical audio model | Wake-word-free Nest Hub Max, no accuracy published | https://research.google/blog/look-and-talk-natural-conversations-with-google-assistant/ |
| Selective Attention System | attenlabs, 2026 | Audio, optional camera, interaction history | F1 0.86 audio only, under 20 MB, under 150 ms on ARM Cortex-A, cloud service only | https://arxiv.org/abs/2604.08412 |
| Continuous address levels in multi-party dialogue | Kyoto, 2026 | Turn taking, gaze, backchannels | Address is graded, which supports a 0 to 100 score over a binary label | https://arxiv.org/abs/2607.15648 |
| Open datasets | Hey Snips, VACC | Wake word, German lab recordings | Academic use only, not addressee data | https://github.com/sonos/keyword-spotting-research-datasets |

### Components

| Name | Repo | Language | License | Last release | Use | Reason | Pin |
|---|---|---|---|---|---|---|---|
| sherpa-onnx (official Rust crate) | https://github.com/k2-fsa/sherpa-onnx | Rust over C++ | Apache-2.0 | 1.13.8, 2026-09-11 | embed as crate | One crate for Silero VAD, TEN VAD and speaker embeddings, static-links the prebuilt library, maintained upstream | `1.13.8` |
| sherpa-rs | https://github.com/thewh1teagle/sherpa-rs | Rust over C++ | MIT | 0.6.8, 2025-10-05 | skip | Same C API with `speaker_id`, `silero_vad` and `ten_vad` modules, deprecated upstream in favour of the official crate | skip |
| Silero VAD | https://github.com/snakers4/silero-vad | ONNX model | MIT | v6.2.3, 2026-09-23 | model through sherpa-onnx | 2.2 MB ONNX, under 1 ms per 32 ms chunk on one CPU thread, AUC 0.952 on the TEN public set in an independent card | model file hash |
| voice_activity_detector | https://github.com/nkeenan38/voice_activity_detector | Rust | MIT | 0.2.1, 2025-08-04 | keep as alternative | Silero v5 through `ort` 2.0.0-rc.10, which pins an older `ort` than the router | `0.2.1` |
| earshot | https://github.com/pykeio/earshot | Rust | MIT OR Apache-2.0 | 1.2.2, 2026-08-19 | embed as crate (cheap gate) | Pure Rust, about 2 us per frame, author claims accuracy above Silero v6, the WebRTC-style family scores AUC 0.760 against Silero 0.894 on AMI in the independent card, so it gates and Silero decides | `1.2.2` |
| TEN VAD | https://github.com/TEN-framework/ten-vad | C, ONNX | Apache-2.0 with extra conditions | 2025-06 | skip | License bars deployments that compete with Agora, Linux aarch64 not listed, vendor-only accuracy claims | skip |
| webrtc-vad | https://github.com/kaegi/webrtc-vad | Rust over C | MIT | 0.4.0, 2019-10-01 | skip | Unmaintained, earshot covers the same model in pure Rust | skip |
| 3D-Speaker CAM++ | https://github.com/modelscope/3D-Speaker | ONNX through sherpa-onnx | Apache-2.0 | 2025 | model through sherpa-onnx | 7.2M parameters, 29 MB ONNX, EER 0.65% on VoxCeleb1-O, about 2x faster than ECAPA on one thread | ONNX file hash from the sherpa-onnx speaker release |
| WeSpeaker ResNet34 | https://github.com/wenet-e2e/wespeaker | ONNX through sherpa-onnx | Apache-2.0 code, CC-BY-4.0 weights | 2025 | keep as alternative | 26.5 MB ONNX, EER 0.72% | ONNX file hash |
| NVIDIA TitaNet-L | https://huggingface.co/nvidia/speakerverification_en_titanet_large | ONNX through sherpa-onnx | CC-BY-4.0 | 2023 | keep as alternative | 101 MB ONNX, EER 0.68% | ONNX file hash |
| ECAPA-TDNN (SpeechBrain) | https://huggingface.co/speechbrain/spkrec-ecapa-voxceleb | Python | Apache-2.0 | 2024 | sidecar only | EER 0.80%, no shipped ONNX, a Python process costs about 300 to 500 MB RSS on a Pi | skip |
| Qwen3 0.6B | https://huggingface.co/Qwen/Qwen3-0.6B | weights | Apache-2.0 | 2025-04-29 | embed through Ollama | Smallest instruction model that follows a schema, thinking toggle, logprobs through the native API | `qwen3:0.6b` |
| oww-rs | https://github.com/skoky/oww_rs | Rust | MIT | 0.3.3, 2026-06-12 | embed as crate (wake word fallback) | openWakeWord inference through `tract-onnx`, no Python at run time | `0.3.3` |
| openWakeWord | https://github.com/dscripka/openWakeWord | Python | Apache-2.0 code, CC BY-NC-SA 4.0 pretrained models | 0.6.0, 2024-02 | train once in Python | Trains a custom word from synthetic speech, a Pi 3 core runs 15 to 20 models in real time | model file hash |
| rustpotter | https://github.com/GiviMAD/rustpotter | Rust | Apache-2.0 | 3.0.2, 2023-10-01 | skip | Author says not production grade, unmaintained since 2023 | skip |
| Porcupine | https://github.com/Picovoice/porcupine | C with bindings | Apache-2.0 SDK, proprietary engine | Rust SDK unmaintained since 2025-07 | skip | Needs an access key validated over the internet, free tier ended 2026-06-30, `pv_porcupine` is yanked | skip |
| microWakeWord | https://github.com/kahrendt/microWakeWord | Python training, tflite-micro | Apache-2.0 | 2025 | skip | Built for ESP32, no Rust runtime | skip |
| L2CS-Net gaze | https://github.com/Ahmednull/L2CS-Net | PyTorch, ONNX export | MIT code, weights unstated | 2022 | later, through `ort` | A coarse "toward the screen" feature once a camera exists, MediaPipe has no Rust binding | skip in v0 |

## Recommendation

```toml
# crates/mote-attention/Cargo.toml
[dependencies]
sherpa-onnx = { git = "https://github.com/k2-fsa/sherpa-onnx", tag = "v1.13.8" }
earshot = { git = "https://github.com/pykeio/earshot", tag = "v1.2.2" }
mote-localmodel = { path = "../mote-localmodel" }
serde = { version = "=1.0.228", features = ["derive"] }
serde_json = "=1.0.151"

[dependencies.oww-rs]
git = "https://github.com/skoky/oww_rs"
tag = "v0.3.3"
optional = true
```

Replace each `tag` with the exact `rev` after `git ls-remote --tags` confirms the tag name. Model files come from the sherpa-onnx speaker release and the Silero release, fetched once by `moted attention fetch` and checked by hash.

The scorer request through `mote_localmodel::ChatRequest`:

```json
{"model":"qwen3:0.6b","stream":false,"think":false,"keep_alive":"-1","logprobs":true,"top_logprobs":4,
 "options":{"temperature":0,"num_predict":40},
 "format":{"type":"object","required":["addressed","sureness","reason"],
  "properties":{"addressed":{"type":"boolean"},"sureness":{"type":"integer","minimum":0,"maximum":100},"reason":{"type":"string","maxLength":120}}},
 "messages":[{"role":"system","content":"You decide whether the last line is spoken to the assistant named mote. Answer with the JSON object only."},
             {"role":"user","content":"[owner 12:01:03] can you check the build\n[other 12:01:10] i think it is the probe again\n[owner 12:01:14] mote, is it still red"}]}
```

The parser is a `serde` struct with `deny_unknown_fields`, `sureness` as `u8` checked to be at most 100, and a trailing-text check. A parse failure records `sureness: 0` with reason `parse_error`. When `logprobs` are present the calibrated score replaces the model's `sureness`: take the first content token after `"addressed":`, read `logit P(true) - logit P(false)`, apply the fitted temperature, and map to 0 to 100.

Fusion default: when the speaker score is at or above the enrolment threshold the text sureness passes through. When it is below, the sureness is capped at the lower edge of the ask band. The cap is a setting.

Enrolment: `moted attention enrol` records three 5 s samples, averages the CAM++ embeddings and stores one 192-dim vector in the context store. Only the vector is kept.

## Efficiency targets and what to measure

| Stage | Mac | Pi 5 | How measured |
|---|---|---|---|
| VAD per 32 ms chunk | < 0.5 ms | < 2 ms (approx.) | `moted bench attention --vad` over 60 s of fixture audio |
| Speaker embedding per 3 s utterance | < 30 ms | < 200 ms (approx.) | same bench |
| Text verdict, 60-token prompt, 0.6B | < 150 ms | < 1500 ms | same bench, 20 utterances |
| End of speech to verdict | < 400 ms | < 2000 ms with the LLM, < 500 ms with the text head | event log timestamps |
| Idle CPU with VAD only | < 3% | < 10% | `top` over 60 s |
| Calibration | ECE <= 0.10 | same | `moted bench attention --calibrate` on the labelled set |
| Resident RAM | < 1 GiB | < 700 MiB | `ollama ps` plus process RSS |

Published anchors: Qwen3 0.6B through Ollama on a Pi 5 does prompt eval at 61.8 t/s and decode at 16.6 t/s, so a 200-token prompt costs about 3.5 s. On an M4 the same model does 2063 t/s prompt eval and 168 t/s decode. Pi 5 timings for Silero and CAM++ are unpublished.

## Open decisions with defaults

1. VAD. Default: Silero v6 through `sherpa-onnx`, with `earshot` as the always-on cheap gate in front of it. Earshot alone when the fixture set shows no accuracy loss.
2. Speaker model. Default: 3D-Speaker CAM++. WeSpeaker ResNet34 when CAM++ misses the fixture EER.
3. Text scorer. Default: `qwen3:0.6b`, structured output, thinking off, calibrated from `logprobs`. The verbalized `sureness` is the fallback when `logprobs` are missing from a streaming response.
4. Calibration method. Default: temperature scaling. Isotonic regression once more than 500 labels exist.
5. Fusion. Default: pass-through gated by the speaker score, cap at the ask band's lower edge for an unknown voice.
6. Bands. Default: answer at 75 and above, ask from 40 to 75, silent below 40. All three are settings.
7. Pi text path. Default: the router's `model2vec-rs` plus logistic head on the transcript window once 200 labelled verdicts exist, the LLM with a 60-token prompt before that.
8. Wake word. Default: off. `oww-rs` with a word trained in openWakeWord's Python pipeline when on. Pretrained openWakeWord models are non-commercial, which fits personal use only.
9. Camera gaze. Default: not in v0.
10. Labels. Default: an owner action on each verdict card, an answer to the ask question, and a repeat with the wake word after silence counts as a missed address.

## Acceptance criteria

- **ATT-01** VAD segments a fixture recording into utterances with start and end within 100 ms of the reference labels. **Verify:** `cargo test -p mote-attention vad::` on the fixture set.
- **ATT-02** Enrolment stores one owner embedding and verification scores every owner sample above every stranger sample in the fixture set. **Verify:** `cargo test -p mote-attention speaker::` with two recorded voices.
- **ATT-03** The text scorer returns one JSON verdict and any malformed output scores 0 with reason `parse_error`. **Verify:** unit tests feed empty, prose, truncated, extra-field and out-of-range outputs.
- **ATT-04** Fusion yields a sureness from 0 to 100 and the three bands map to answer, ask and silent with thresholds read from settings. **Verify:** scripted three-band test with the thresholds changed between runs.
- **ATT-05** On the labelled set the expected calibration error is at most 0.10 and the report is in the evidence file. **Verify:** `moted bench attention --calibrate`.
- **ATT-06** The detector opens no network socket and writes no memory row. **Verify:** `lsof -p` during a live session and a memory table diff before and after.
- **ATT-07** Each verdict row names the utterance id, speaker score, text score, sureness, band and reason. **Verify:** `SELECT` over the event log after a live session.
- **ATT-08** End of speech to verdict p50 is under 400 ms on the Mac. **Verify:** bench output in the evidence file.
- **ATT-09** A wake word, when enabled, sets sureness to 100 for the next utterance. **Verify:** scripted test with the wake word fixture.
- **ATT-10** No raw audio is stored. Only utterance ids and redacted text reach the data directory. **Verify:** list the data directory after a session and grep the event log for audio blobs.
- **ATT-11** The ask action produces one short question and never a second question before the owner answers. **Verify:** scripted test with two ambiguous utterances in a row.
- **ATT-12** When `logprobs` are present the calibrated score is used and the ledger row records both the raw logit gap and the calibrated value. **Verify:** `SELECT` after a live session plus a unit test with a fixture response.

## Sources

- Shriberg et al. 2012: https://www.isca-archive.org/interspeech_2012/shriberg12_interspeech.html
- Mallidi et al. 2018: https://arxiv.org/abs/1808.02504
- Norouzian et al. 2019: https://arxiv.org/abs/1902.00570
- Gillespie et al. 2020: https://arxiv.org/abs/2010.01949
- Apple streaming on-device 2022: https://machinelearning.apple.com/research/streaming-on-device
- Dighe et al. 2023: https://arxiv.org/abs/2309.04842
- Apple multimodal 2024: https://arxiv.org/abs/2403.14438 and https://arxiv.org/abs/2312.03632
- Apple follow-up 2024: https://arxiv.org/abs/2411.00023
- Google Look and Talk: https://research.google/blog/look-and-talk-natural-conversations-with-google-assistant/
- attenlabs SAS: https://arxiv.org/abs/2604.08412 and https://github.com/attenlabs/saa-sdk
- Continuous address: https://arxiv.org/abs/2607.15648
- Hey Snips dataset: https://github.com/sonos/keyword-spotting-research-datasets
- Calibration: https://arxiv.org/abs/1706.04599, https://arxiv.org/abs/2207.05221, https://arxiv.org/abs/2305.14975, https://arxiv.org/abs/2407.01122, https://arxiv.org/abs/2410.02210
- Reject option and clarification: https://arxiv.org/abs/2107.11277, https://arxiv.org/abs/2311.09469
- Implicit feedback labels: https://arxiv.org/abs/1911.02557
- Ollama logprobs: https://docs.ollama.com/api/chat and https://github.com/ollama/ollama/releases?q=logprobs
- llama-server logprobs: https://github.com/ggml-org/llama.cpp/pull/10783
- Pi 5 Qwen3 0.6B: https://learn.adafruit.com/local-llms-on-raspberry-pi/qwen3
- M4 Qwen3 0.6B: https://omlx.ai/benchmarks/vgpenr7w
- Silero VAD: https://github.com/snakers4/silero-vad/releases
- Independent VAD card: https://huggingface.co/Teensy/teensy-vad-3
- earshot: https://crates.io/crates/earshot
- TEN VAD license: https://github.com/TEN-framework/ten-vad/blob/main/LICENSE
- sherpa-onnx crate: https://crates.io/crates/sherpa-onnx and speaker models https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-recongition-models
- sherpa-rs: https://docs.rs/sherpa-rs
- 3D-Speaker and CAM++: https://github.com/modelscope/3D-Speaker and https://arxiv.org/abs/2303.00332
- WeSpeaker: https://github.com/wenet-e2e/wespeaker/blob/master/docs/pretrained.md
- TitaNet: https://huggingface.co/nvidia/speakerverification_en_titanet_large
- openWakeWord and oww-rs: https://github.com/dscripka/openWakeWord and https://crates.io/crates/oww-rs
- Porcupine free tier end: https://community.home-assistant.io/t/fyi-picovoice-confirmed-free-tier-accesskeys-will-stop-working-after-june-30-2026/1012744
- microWakeWord: https://github.com/kahrendt/microWakeWord
- L2CS-Net: https://github.com/Ahmednull/L2CS-Net
- EU AI Act articles 5 and 50: https://artificialintelligenceact.eu/article/5/ and https://artificialintelligenceact.eu/article/50/
