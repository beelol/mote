# Reuse: owned hardware path

Concept 11 in `docs/concepts.md`. The Pi renderer sits behind DEF-01 and the relay behind
DEF-02 in `docs/rfcs/mote.md`. This document fixes the parts, the operating system, the input
and display stack, the cross-compile path from the Mac, the relay's trust boundary and what a
phone client needs. The rendering pipeline itself is the renderer agent's work
(`docs/handoff/generative-ui-renderer.md`). Research date: 2026-10-08. Prices are US list
prices after the April 2026 memory-cost increases and will move.

## Key points

1. The Pi is a client of the Mac's `moted` over the gateway first. A local talk lane on the Pi is an experiment, not a requirement.
2. Buy a Raspberry Pi 5. Pi 4 is slower and has no PCIe. CM5 is the same silicon but needs a carrier and does not autodetect the display.
3. 8 GB ($175) is enough for a client plus a 3B model. 16 GB ($305) is the only way to hold a 7B model, which decodes at 1 to 2.5 tokens per second on the CPU.
4. Display: Raspberry Pi Touch Display 2, 7 inch, 720 by 1280, DSI, five-point capacitive touch, powered from the GPIO header, $60. Native portrait.
5. No desktop. Raspberry Pi OS Lite 64-bit (Trixie) with the default KMS driver. Pure-Rust `drm` and `evdev` crates need no system libraries and cross-compile from the Mac with `cargo zigbuild`.
6. Audio needs hardware echo cancellation. A USB speakerphone (Jabra Speak 510) or a ReSpeaker XVF3800 USB array does it on the device. Linux full-duplex behaviour of the Jabra is unverified.
7. Local inference throttles within about 90 seconds without the Active Cooler ($5). The 27 W PD supply is required for the 1.6 A USB budget.
8. The AI HAT+ 2 ($130, Hailo-10H, 8 GB on board) runs only Hailo's own 1B to 1.5B models through `hailo-ollama` on port 8000. Prefill speed is unknown. Skip until measured.
9. A relay forwards opaque Noise frames and sees only the gateway fingerprint, ephemeral keys, addresses, timing and sizes. Overseer's phone client (`phone/core`, TypeScript, Expo 57) already speaks the protocol and mote reuses it with a different prologue and code prefix.

## What the concept needs technically

**Board.** Pi 5: BCM2712, four Cortex-A76 at 2.4 GHz, LPDDR4X-4267, VideoCore VII with
OpenGL ES 3.1 and Vulkan 1.3, two 4-lane MIPI connectors usable as DSI or CSI, PCIe 2.0 x1,
two USB 3.0 and two USB 2.0, RTC, power button. Pi 4: Cortex-A72 at 1.8 GHz, one 15-pin DSI,
no PCIe, no RTC. CM5: Pi 5 silicon on a module, 1 to 16 GB, needs the CM5 IO Board, no DSI
autodetect, current price unknown after the 2026 increases.

**Display.** Touch Display 2 (7 inch, SC1635): 720 by 1280 at 60 Hz, 24-bit, five touch
points, 15 mm deep, 22-way to 15-way FFC to the Pi 5, 5 V and ground from GPIO pins 2 and 6.
Pi 5 autodetects it. For landscape the renderer rotates its own frame. The console rotates with
`video=DSI-1:720x1280@60,rotate=90` in `/boot/firmware/cmdline.txt`. Touch axes rotate with
`dtoverlay=vc4-kms-dsi-ili9881-7inch,swapxy,invx` or a libinput calibration matrix. The 5 inch
model has the same resolution for $40. The 10 inch model is 1200 by 1920 and $100 class.

**No desktop.** Raspberry Pi OS Lite 64-bit, Trixie (Debian 13, kernel 6.12, glibc 2.41).
`dtoverlay=vc4-kms-v3d` is the default. The display node is `/dev/dri/card*` with connector
`DSI-1`, render nodes are owned by group `render`, input devices by `input`. The first user
Imager creates is in `video`, `render` and `input`. Verify with `id` and `ls -l /dev/dri`.
Software rendering uses DRM dumb buffers and needs no Mesa. GPU rendering needs `libgbm`,
`libEGL` and Mesa's `v3d` driver, which means a sysroot when cross-compiling.

**Input.** evdev exposes the touch panel as a multitouch device at `/dev/input/event*`.
`evdev` 0.13 reads it in pure Rust. `input` 0.10 binds libinput and adds gesture and calibration
handling at the cost of a system library. `libseat` 0.2.4 avoids running as root.

**Audio.** Full duplex needs acoustic echo cancellation with a reference of the played signal.
Devices that do it on board: Jabra Speak 410 and 510 (USB speakerphone, AEC in the DSP, Jabra
says Linux works "at the audio level"), ReSpeaker XVF3800 USB 4-mic array (AEC, AGC, DoA, UAC
2.0, I2S option, about £48), ReSpeaker Lite (2 mics, XU316, UAC 2.0, 16 kHz max, cheapest).
Software fallback: `webrtc-audio-processing` 2.1.0 on the Pi CPU. USB power: 600 mA total on a
3 A supply. A reseller states 1.6 A once a 5 A PD supply is detected. Verify on the device with
`vcgencmd get_throttled` and `vcgencmd pmic_read_adc EXT5V_V`.

**Local models on a Pi 5 (CPU, Q4_K_M).** Reported numbers: 0.5B about 19 tokens per second.
1.5B 10 to 15. 3B 5 to 7 decode and about 20 prompt tokens per second. 7B 1 to 2.5. A 30B MoE
at 2.7 bits reached 8 on a 16 GB board in one forum report. llama.cpp direct is 10 to 20%
faster than Ollama. Prompt processing is the real limit: a 3000-token pack on a 3B model takes
about 150 seconds cold. MOTE-04's 300 ms first token is out of reach on the Pi CPU. 16 GB does
not make anything faster. It only lets 7B to 14B models fit. Ollama ships arm64 Linux builds.

**AI HAT+ 2.** Hailo-10H, 40 TOPS INT4, 8 GB on board, PCIe, Pi 5 only, launched 2026-01-15
at $130. Needs Trixie, `dkms`, `hailo-h10-all` and Hailo's Gen-AI Model Zoo 5.1.1 Debian
package. `hailo-ollama` serves Ollama-style `/api/pull` and `/api/chat` on port 8000. Only
Hailo-compiled models run (Llama 3.2 1B, Qwen2.5 1.5B, DeepSeek R1 1.5B reported). Decode
figures conflict: 20 to 50 tokens per second in one blog, 7 to 9 in a benchmark catalogue.
Prefill speed: unknown. Custom models cannot be compiled by users. The original AI HAT+ (Hailo-8L
13 TOPS, Hailo-8 26 TOPS) does not run LLMs.

**Cross-compiling from the Mac.** `cargo-zigbuild` 0.23.4 and Zig 0.16.0 are installed, and
the `aarch64-unknown-linux-gnu` target is present. Zig bundles glibc, so pin the floor with
`--target aarch64-unknown-linux-gnu.2.36` (Bookworm) and it runs on Trixie (2.41). Static glibc
is unsupported. `cross` 0.2.5 is from 2023, needs Docker, and its maintainers point at `main`.
Crates that link system libraries (`input`, `gbm`, Slint with fontconfig) need a sysroot or a
native build on the Pi.

**Relay (DEF-02).** Overseer's protocol says the session layer does not depend on the
transport. Noise IK message 1 sends the device's ephemeral key in clear and its static key
encrypted. The relay therefore learns an unlinkable ephemeral key, the gateway fingerprint used
for routing (already public in the mDNS TXT record), peer addresses, timing and sizes. It must
not learn plaintext, either static key, the pairing secret, device names, counters or request
ids. Pairing stays on the LAN because it needs the Mac's confirmation and the QR code. The
counter in message 1 defeats a replaying relay. The relay must refuse unknown gateway
fingerprints, because both Tailscale and iroh document open-relay abuse when access is not
verified. Tailscale's DERP is the design reference: packets addressed by public key, relay holds
no private key, falls back only when hole punching fails.

**Phone client.** Overseer's `phone/` is Expo 57 and React Native 0.86. `phone/core` implements
Noise IK and IKpsk1 in TypeScript on `@noble/ciphers`, `@noble/curves` and `@noble/hashes` 2.4.0,
checked against the shared vectors. It needs a keystore (`expo-secure-store`), a QR scanner
(`expo-camera`), discovery by fingerprint, cursor replay, an outbox with request ids, and push.
Overseer sends iOS push straight to Apple with its own key. Android push needs Firebase and
comes with the relay.

## Candidates

| Name | Repo or product | Lang | License | Last release or availability | Verdict | Reason | Pin or part |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Raspberry Pi 5 8 GB | raspberrypi.com | hw | n/a | $175 (2026-04) | buy | client plus a 3B model | Pi 5 8 GB |
| Raspberry Pi 5 16 GB | raspberrypi.com | hw | n/a | $305 (2026-04) | buy only for 7B experiments | memory is soldered, no upgrade path | SC1113 |
| Raspberry Pi 4 | raspberrypi.com | hw | n/a | 4 GB about $100 | skip | A72, one DSI, no PCIe | n/a |
| Compute Module 5 + IO Board | raspberrypi.com | hw | n/a | price unknown after 2026 rises | skip | same silicon, carrier, no DSI autodetect | n/a |
| Touch Display 2, 7 inch | raspberrypi.com/products/touch-display-2 | hw | n/a | $60, in production to 2030 | buy | DSI, 5-point touch, GPIO power | SC1635 |
| Touch Display 2, 5 inch | same | hw | n/a | $40 | alternative | same resolution, smaller | SC16xx (unknown) |
| Raspberry Pi OS Lite 64-bit | raspberrypi.com/software | OS | mixed | Trixie, 2025-10 | buy (use) | KMS by default, no desktop | Trixie |
| drm | github.com/Smithay/drm-rs | Rust | MIT | 0.15.0, 2026-03-19 | embed | pure-Rust KMS and dumb buffers | `0.15.0` |
| evdev | github.com/cmr/evdev | Rust | MIT or Apache-2.0 | 0.13.2, 2025-09-15 | embed | pure-Rust touch input | `0.13.2` |
| input (libinput) | github.com/Drakulix/input.rs | Rust | MIT | 0.10.0, 2026-04-05 | sidecar option | gestures and calibration, needs libinput | `0.10.0` |
| libseat | github.com/Smithay/libseat.rs | Rust | MIT | 0.2.4, 2025-12-07 | sidecar option | unprivileged device access | `0.2.4` |
| Slint linuxkms backend | github.com/slint-ui/slint | Rust | GPL-3.0 or Slint royalty-free or commercial | 1.18.1, 2026-09-21 | port the idea | royalty-free tier excludes embedded, backend marked experimental | `1.18.1` |
| Jabra Speak 510 | jabra.com | hw | n/a | listed, some stock gaps | buy (default audio) | speaker, mics and AEC in one USB device | Speak 510 |
| ReSpeaker XVF3800 USB array | seeedstudio.com | hw | n/a | 2025-10, about £48 | alternative | on-chip AEC, UAC 2.0, I2S | SKU 106619 |
| ReSpeaker Lite | seeedstudio.com | hw | n/a | available | alternative, cheapest | 2 mics, XU316 AEC, 16 kHz | reSpeaker Lite USB |
| webrtc-audio-processing | crates.io | Rust | check (listed non-standard) | 2.1.0, 2026-05-13 | sidecar fallback | software AEC if the device has none | `2.1.0` |
| Waveshare PI5-CASE-TD2 | waveshare.com | hw | n/a | $8.99 | buy | encloses Pi 5 and the 7 inch panel, tripod thread | PI5-CASE-TD2 |
| 27 W USB-C PD supply | raspberrypi.com | hw | n/a | available | buy | 5.1 V 5 A, unlocks the USB budget | SC1152 |
| Active Cooler | raspberrypi.com | hw | n/a | $5 | buy | sustained inference throttles without it | SC1148 |
| AI HAT+ 2 | raspberrypi.com/products/ai-hat-plus-2 | hw | n/a | $130, 2026-01-15 | skip for now | closed model list, prefill unknown | AI HAT+ 2 |
| Ollama arm64 | github.com/ollama/ollama | Go | MIT | installed on the Mac | sidecar on the Pi | same API as the Mac talk lane | match the Mac version |
| cargo-zigbuild | github.com/rust-cross/cargo-zigbuild | Rust | MIT | 0.23.4, 2026-09-02 | buy (use) | installed, no Docker | `0.23.4`, Zig 0.16.0 |
| cross | github.com/cross-rs/cross | Rust | MIT or Apache-2.0 | 0.2.5, 2023-02-04 | skip | needs Docker, stale release | git rev `74e99b2` if ever used |
| snow | github.com/mcginty/snow | Rust | Apache-2.0 or MIT | 0.10.0, 2025-07-19 | embed (via mote-gateway) | Overseer's Noise, revision 34, unaudited | `0.10` |
| tokio-tungstenite | github.com/snapview/tokio-tungstenite | Rust | MIT | 0.30.0, 2026-07-11 | embed (gateway and relay) | Overseer pins 0.30 | `0.30` |
| iroh | github.com/n0-computer/iroh | Rust | MIT or Apache-2.0 | 1.3.0, 2026-09-28 | skip for now | QUIC with its own identity, relay sees metadata, double encryption with Noise | `1.3.0` |
| Tailscale DERP | tailscale.com/kb/1232/derp-servers | Go | BSD-3-Clause | current | port the idea | key-addressed relay, no private keys | n/a |
| Overseer phone/core | ~/projects/overseer phone/core | TS | MIT per manifests | branch `1488b4be` | port (reuse) | Noise client, vectors, outbox | `1488b4be` |

## Recommendation

Parts list, one unit:

| Part | Number | Price |
| --- | --- | --- |
| Raspberry Pi 5, 8 GB | Pi 5 8 GB | $175 |
| Touch Display 2, 7 inch | SC1635 | $60 |
| Active Cooler | SC1148 | $5 |
| 27 W USB-C PD power supply | SC1152 | $12 class |
| Waveshare case for Pi 5 and 7 inch TD2 | PI5-CASE-TD2 | $9 |
| microSD 64 GB A2 or an NVMe later | any | $15 |
| Jabra Speak 510 (optional until DEF-03) | Speak 510 | $100 class, varies |
| Total without audio | | about $276 |

Choose 16 GB only if the owner wants to run 7B models on the Pi, knowing decode is 1 to 2.5
tokens per second.

Pi crate dependencies, pure Rust so the Mac can build them:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
drm = "0.15.0"
evdev = "0.13.2"
libseat = { version = "0.2.4", optional = true }
input = { version = "0.10.0", optional = true }
```

Build and deploy from the Mac:

```sh
rustup target add aarch64-unknown-linux-gnu
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36 -p moted
scp target/aarch64-unknown-linux-gnu/release/moted pi@mote-pi.local:~/
ssh pi@mote-pi.local 'id; ls -l /dev/dri /dev/input; cat /sys/class/drm/card*-DSI-1/status'
```

Pi setup, once:

```sh
sudo rpi-eeprom-update -a && sudo apt update && sudo apt full-upgrade
echo 'video=DSI-1:720x1280@60,rotate=90' | sudo tee -a /boot/firmware/cmdline.txt
sudo systemctl set-default multi-user.target
curl -fsSL https://ollama.com/install.sh | sh   # only after the owner approves a model pull
```

Relay, later: a small `tokio-tungstenite` 0.30 server that maps a gateway fingerprint to one
outbound gateway connection and forwards frames byte for byte. Both sides dial out. The gateway
keeps the same `snow` session code. Clients present the fingerprint in the URL path only.

## Efficiency targets and what to measure

| Target | Number | Measure with |
| --- | --- | --- |
| Idle CPU with the UI on screen | under 3% | `top -b -n 5` average |
| Idle RSS of the renderer process | under 100 MB | `ps -o rss` |
| Frame rate on scroll, software path | 30 fps minimum, 60 fps target | frame timing log |
| Part received to first paint | under 50 ms | renderer log |
| Touch down to visual response | under 100 ms | renderer log plus a 240 fps phone video |
| Boot to UI | under 25 s from power on | stopwatch and `systemd-analyze` |
| Power at idle | under 4 W | USB-C inline meter |
| Thermal under 10 min inference | under 80 C, `get_throttled` 0x0 | `vcgencmd measure_temp` loop |
| Binary size | under 20 MB stripped | `ls -l` |
| Gateway reconnect after a 60 s cut | under 5 s, zero duplicate events | CRATE-07 test on the Pi |
| Local model prefill | tokens per second per model, decides the talk lane | `moted bench talk` |
| Echo attenuation | at least 20 dB on the chosen device | loopback recording and RMS script |

## Open decisions with defaults

1. RAM. Default: 8 GB. 16 GB only for 7B experiments.
2. Display size. Default: 7 inch. 5 inch if the desk spot is small.
3. Orientation. Default: landscape 1280 by 720, rotation done in the renderer, not in the kernel, so the console setting is only for the login prompt.
4. Input stack. Default: `evdev` pure Rust. `input` plus `libseat` only if gestures or multi-seat are needed.
5. Rendering path. Default: DRM dumb buffers and CPU rasterization, so no sysroot. GPU via Vulkan 1.3 is the renderer agent's call.
6. Audio device. Default: Jabra Speak 510 as one box. ReSpeaker XVF3800 if the owner wants direction of arrival or a separate speaker.
7. Local model on the Pi. Default: none. The Pi is a client. Reopen after `moted bench talk` numbers exist.
8. AI HAT+ 2. Default: skip. Reopen if Hailo publishes prefill numbers or opens model compilation.
9. Enclosure. Default: Waveshare case. Pibow if the Pi should stay reachable.
10. Cross-compile tool. Default: `cargo zigbuild` with the `.2.36` glibc suffix. `cross` only if a system-library crate becomes unavoidable.
11. Relay implementation. Default: own 300-line WebSocket forwarder behind DEF-02. iroh if hole punching becomes a requirement.
12. Phone client. Default: reuse Overseer `phone/core` with prologue `mote-gateway-v1` and prefix `MOTE1` as configuration. A Rust core through `uniffi` only if the Expo app is retired.

## Acceptance criteria

- **HW-01** The owner approves the parts list above or an amended one, and the amendment is recorded in this document. **Verify:** DEF-01 checked in the README and this section updated with the final part numbers.
- **HW-02** A `moted` binary built on the Mac with `cargo zigbuild --target aarch64-unknown-linux-gnu.2.36` runs on Raspberry Pi OS Lite Trixie with no extra packages. **Verify:** `ldd moted` and `moted --version` output on the Pi in the evidence record.
- **HW-03** The Pi boots to `multi-user.target` and mote owns `DSI-1` at 720 by 1280 through DRM/KMS with no X11 or Wayland process. **Verify:** `ps -e | grep -Ei 'xorg|wayland|labwc'` empty, `cat /sys/class/drm/card*-DSI-1/status` connected, and a photo.
- **HW-04** Touch events arrive through evdev as multitouch with axes mapped to the landscape frame, and a 44-pixel target is hit at all four corners. **Verify:** a touch log from the daemon and a photo per corner.
- **HW-05** Idle CPU stays under 3% and the renderer RSS under 100 MB with a surface on screen, and scrolling holds at least 30 fps on the software path. **Verify:** `top` samples and the frame timing log.
- **HW-06** The Pi client reconnects to the Mac's gateway after a 60 s Wi-Fi cut, resumes from its cursor, and shows nothing twice. **Verify:** the CRATE-07 client test run on the Pi plus one live cut with the event log compared.
- **HW-07** With display, cooler and the audio device attached, a 10 minute scroll and playback run reports no undervoltage. **Verify:** `vcgencmd get_throttled` prints `0x0` and `pmic_read_adc EXT5V_V` stays above 4.8 V.
- **HW-08** Ten minutes of sustained local inference stays under 80 C with the Active Cooler and no throttle flag. **Verify:** a `measure_temp` loop and `get_throttled` in the evidence record.
- **HW-09** `moted bench talk` on the Pi reports prefill and decode tokens per second for a 1.5B and a 3B model, and open decision 7 is resolved from the numbers. **Verify:** bench output in the evidence record and the decision line updated.
- **HW-10** Full-duplex capture while playing a tone shows at least 20 dB of echo attenuation from the chosen device's hardware AEC. **Verify:** `arecord` while `aplay`, RMS measured by a script, output in the evidence record.
- **HW-11** A relay threat table lists what the relay sees and never sees, a test relay forwards opaque frames end to end, and a hostile relay that replays message 1 is refused by the counter check. **Verify:** `cargo test -p mote-gateway relay` with a mock relay and a replayed handshake.
- **HW-12** Overseer's `phone/core` client pairs with mote's gateway using prologue `mote-gateway-v1` and prefix `MOTE1` and passes the mote Noise vectors. **Verify:** the vectors test in `phone/core` against `crates/mote-gateway/tests/vectors/mote-noise.json` and a live pairing screenshot.

## Sources

- Raspberry Pi 5 product page: https://www.raspberrypi.com/products/raspberry-pi-5/
- Raspberry Pi hardware documentation (power, fan header, MIPI): https://www.raspberrypi.com/documentation/computers/raspberry-pi.html
- Touch Display 2 documentation: https://www.raspberrypi.com/documentation/accessories/touch-display-2.html
- Touch Display 2 product page: https://www.raspberrypi.com/products/touch-display-2/
- AI HAT+ and AI HAT+ 2 documentation: https://www.raspberrypi.com/documentation/accessories/ai-hat-plus.html
- AI software, `hailo-ollama` setup: https://www.raspberrypi.com/documentation/computers/ai.html
- AI HAT+ 2 launch coverage: https://www.phoronix.com/news/Raspberry-Pi-AI-HAT-2 and review: https://www.tomshardware.com/raspberry-pi/raspberry-pi-ai-hat-plus-2-review
- Hailo-10H LLM throughput (conflicting): https://raspberry.tips/en/?p=9674 and https://codesota.com/embedded-ai/hailo-10h-llms
- 2026 price increases: https://www.tweaktown.com/news/110841/raspberry-pi-hikes-prices-by-up-to-dollars150-tries-to-cushion-the-blow-with-new-raspberry-pi-4-3gb-variant/index.html
- Compute Module 5 launch: https://linuxiac.com/raspberry-pi-compute-module-5/
- Pi 5 LLM benchmarks: https://bestllmfor.com/best/raspberry-pi-5-llm/ and https://stratospherelinuxips.readthedocs.io/en/develop/immune/research_rpi_llm_performance.html
- Qwen3 30B on a 16 GB Pi 5 (forum): https://dietpi.com/forum/t/qwen-llm-on-raspberry-pi-5-16-gb/24847
- Slint LinuxKMS backend: https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_linuxkms/ and pricing: https://slint.dev/pricing
- drm-rs: https://github.com/Smithay/drm-rs, evdev: https://github.com/cmr/evdev, input.rs: https://github.com/Drakulix/input.rs
- ReSpeaker XVF3800: https://thepihut.com/products/respeaker-xmos-xvf3800-ai-powered-4-mic-array-for-clear-voice-even-in-noise and ReSpeaker Lite: https://wiki.seeedstudio.com/reSpeaker_usb_v3/
- Jabra on Linux (audio level only): https://www.jabra.com/supportpages/jabra-link-230/230-09/faq/do-i-have-audio-and-call-control-on-my-jabra-device-when-i-use-it-with-computer-softphones-on-linux
- Waveshare case: https://www.cnx-software.com/2025/06/04/raspberry-pi-5-and-raspberry-pi-touch-display-2-get-a-10-protective-case/ and Pibow frame: https://shop.pimoroni.com/products/pibow-frame-for-raspberry-pi-touch-display-2
- Active Cooler SC1148: https://export.farnell.com/raspberry-pi/sc1148/raspberry-pi-5-active-cooler/dp/4263049
- cargo-zigbuild: https://github.com/rust-cross/cargo-zigbuild and cross: https://github.com/cross-rs/cross
- Overseer phone-remote protocol: ~/projects/overseer/docs/rfcs/phone-remote-protocol.md and design: ~/projects/overseer/docs/rfcs/phone-remote.md
- Tailscale DERP servers: https://tailscale.com/kb/1232/derp-servers and iroh relays: https://docs.iroh.computer/iroh-services/relays/public
- snow: https://github.com/mcginty/snow, uniffi: https://github.com/mozilla/uniffi-rs, llama-cpp-rs: https://github.com/utilityai/llama-cpp-rs
