# Key concepts

Twenty promises, written from the owner's side of the screen. This is the product. The products
are characters that run on mote. mote is the engine. The
engineering map below says which document carries the work behind each promise. Rewritten on
2026-10-08 after the owner said the first list read as machinery.

## The promises

| # | Promise | What that means in use |
| --- | --- | --- |
| 1 | It answers instantly. | A word back in about 300 milliseconds, from your own hardware, even offline. |
| 2 | It knows when you are talking to it. | Sure, it answers. Half sure, it asks. Otherwise it stays out of the way. |
| 3 | It texts like a person. | Sent, delivered, read, typing. The chat you already know, kept from dots. |
| 4 | It has a character you choose. | Voice, tone and name are options, not a fixed persona. |
| 5 | It thinks only when it has to. | Hard questions get a real thinking model. Easy ones cost nothing. |
| 6 | It gets things done, not just answered. | Code, research, errands across your tools, carried end to end. |
| 7 | It keeps going. | Blocked, it looks for another way and lines up alternatives. It waits for you only when it must. |
| 8 | It works with your computer off. | The device stands on its own. When your computer is on, it connects for more power, with options you control. |
| 9 | It asks before acting outward. | Nothing is posted, sent or deleted without your approval. |
| 10 | It knows what it does not know. | When it cannot see something or lacks access, it says so and names what it needs. |
| 11 | It can reach everything you can. | Slack, mail, calendar, files, code, the web. All of it, efficiently, through one context layer. |
| 12 | It remembers properly. | Facts with dates, people, projects, and how they changed. Yours, not hosted. |
| 13 | It can tell you what it knew. | Every answer can show what it saw and what it cost. |
| 14 | It draws its own screens. | A reply is an interface, not a chat bubble. Lists, forms, progress, on a touchscreen or a window. |
| 15 | It lives on a small device. | A Pi with a touchscreen on the desk, a window on the Mac, your phone. One agent. |
| 16 | It never loses a thread. | Close it, switch devices, lose Wi-Fi. It picks up where it was. |
| 17 | It runs on what you already pay for. | Your subscriptions, your hardware. No API keys, no per-token bill. Open source. |
| 18 | It can use a browser. | In the cloud when the computer is off. Through the owner's own signed-in sessions when it is on. It never submits, buys or sends without asking. |
| 19 | It can talk. | Voice in and out, hands free. Later. |
| 20 | You can open the hood while it runs. | Click any stage to see what it is doing and change it, down to the model behind it. |

## Engineering map

The promises above are delivered by these pieces. Each has a reuse document under `docs/reuse/`
and criteria on the README board.

| Piece | Delivers promises | Documents |
| --- | --- | --- |
| Topology: mote-cloud, mote-client, mote-ui, mote-talk | 8, 14, 15, 16 | `rfcs/mote.md` (Topology) |
| Talk lane (local model, Haiku fallback) | 1, 17 | `reuse/talk.md` |
| Attention detector | 2 |
| Chat surface (sent, delivered, read, typing) and characters | 3, 4 | `reuse/attention.md` |
| Router (fail-closed verdict) | 5 | `reuse/router.md` |
| Think lane (Claude Code under your login, tools) | 5, 6, 17 | `reuse/think.md` |
| Work lane (Overseer on the Mac, or the device alone) | 6, 7, 8 | `reuse/work.md` |
| Persistence loop (retry, alternatives, wait only when blocked) | 7 | `reuse/always-on.md` (to extend) |
| Always-on watchers and approval gates | 7, 9 | `reuse/always-on.md` |
| Context layer (connectors, index, retrieval, consumption, coverage map) | 10, 11 | `rfcs/context-layer.md` |
| Memory graph | 12 | `reuse/memory.md` |
| Context packs and ledger | 13 | `reuse/context.md` |
| Generative UI and renderer | 14 | `handoff/generative-ui-renderer.md`, `rfcs/ui-language.md` |
| Hardware path and relay | 8, 15 | `reuse/hardware.md` |
| Event log with cursor replay | 16 | `reuse/eventlog.md` |
| Shared crates with Overseer | all, as engineering reuse | `rfcs/shared-crates.md` |
| Scripted-model testing | all, as proof | `reuse/testing.md` |
| Browser use | 6, 11, 18 | `reuse/browser.md` |
| Voice | 19 | `reuse/voice.md` |
| Pipeline view and hot controls | 20 | `rfcs/mote.md` (Visibility and control) |

## Promises that are new today

Promises 7 and 8 come from the owner's words on 2026-10-08: "we just want it to be
able to work with computer off and connect if computer on with options, and be a living IoT
thing that can get shit done and will go until it's blocked and look for alternatives while it
waits." Promises 3, 4, 10, 11 and 18 came later the same day: keep the texting feel from dots, offer
characters, say plainly what it cannot see or reach, and reach all the owner's data through one
context layer, and use a browser the way the owner does. None of the seven has criteria yet. The work lane and always-on documents need a
persistence loop and an offline mode, and the context layer needs its own RFC.
