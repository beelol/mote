# Handoff: generative UI rendering, from language to pixels

For a fresh agent. Nothing here assumes chat history. Date: 2026-10-08.

## What mote is

mote is an always-on personal agent the owner is building at `/Users/bilal/projects/mote`. A
Rust daemon runs on the owner's Mac now, on a Raspberry Pi with a touchscreen later, and in the
cloud eventually. Models emit the user interface for every turn. Read, in this order:

1. `README.md` (the context section and the Ready for Go board).
2. `docs/rfcs/ui-language.md`, key points and sections 5 to 10. It is long. The key points at the top are enough to start.
3. `docs/rfcs/mote.md`, key points and criteria MOTE-14 to MOTE-16.
4. `AGENTS.md`, the writing and code rules.

## What is already decided

- Models never emit HTML, CSS or JavaScript. They emit a validated declarative tree (compact JSON in v0) and later the Mote UI language (`.mote`), which compiles to the same tree.
- No routes and no declared store. The daemon owns data bindings and surface history. The model is the state machine. Events are `-> Name(args)` and go back to the model.
- Built-in affordances are implicit: scrolling, focus, keyboard and touch, loading, error and empty states, light and dark themes, accessibility.
- Streaming: a surface arrives in parts. Text renders as it arrives. Interactive nodes render only when complete.
- The tree carries roles, not pixel units. Layout is a written flexbox subset. Taffy is the reference layout engine.

## What the owner now questions

The UI language RFC proposes a Tauri 2 webview on the Mac and Slint on the Pi. The owner's
words on 2026-10-08:

- "I think web is too heavy frankly for say a small device. I want instantly moving around and animated generative UI."
- "I was thinking we might even work at the pixel level and show it in terminals or arbitrary spaces."
- "Maybe literally it translates our language super fast to pixels."

So the question is no longer "which widget toolkit". It is: design the pipeline that turns the
tree or the language into animated pixels, fast, on every surface mote will ever have.

## Expressiveness: keep the language small (owner, 2026-10-08)

The owner added this after the RFC was written. It narrows section 6 and section 7 of
`docs/rfcs/ui-language.md`. Where the two disagree, this section wins.

The owner's words today: "I don't think it should be super expressive. It should mostly just
work like flexboxes with anchors and doesn't need much customizability beyond the theme."

From an earlier conversation the owner shared, in the owner's own words:

- "Define a design language for UI elements that is purposefully restricting. UI as code. Super simple, no style options except color, palette, theme, and that's baked in."
- "Layout is written script, like switch, toggle, etc. And then typesafe binding between access of things."
- "Data flows through a store and connects via observer pattern, like rxjs or those React competitors. But that's all underneath. UI outputs are standardized."
- "The theme should affect the whole thing, or at least be inherited."
- "I don't think we should even allow overrides. You just have to adhere to the theme for this screen."
- On allowing custom layouts: "maybe for defining new layouts, I guess, but I'm not sure at this level we need it. It's more for agents, and constraints make sense to me for this."

That conversation also sketched a four-layer shape the owner found useful. Treat it as a
sketch to evaluate, not a decision:

| Layer | The author controls |
| --- | --- |
| Elements | text, input, toggle, select, action, list |
| Layout | stack, row, sections, conditional and repeated content |
| Bindings | typed values, writable fields, callable actions |
| Theme | one named palette applied to the whole screen |

Typed bindings from the same sketch: a displayable `Value<T>`, an editable `Field<T>`, a
callable `Action<Input, Result>`. A toggle requires a `Field<bool>`. A select's options must
match its field's type. An action exposes pending and failure states on its own. An invalid
connection fails at validation, never at render. The owner questioned the sketch's idea of
semantic variants such as a "primary" or "destructive" action and leans toward none at all.

What this means for your work:

- Layout is a flexbox subset plus anchors (pin to an edge, safe areas, a bar that stays put). Nothing else. No per-element spacing, sizing, fonts, borders or colors.
- Theme is one token set per screen, inherited by everything, with no overrides and no escape hatch. Decide whether semantic variants exist at all. The default is no.
- Elements are a short closed list. Adding one is a language change, not an author choice.
- Bindings are typed and checked before render. The store and its observer wiring live underneath in the daemon and never appear in what a model writes. This matches the RFC's "no declared store" decision.
- The model emits meaning, layout and bindings. The renderer owns everything else.
- Reconcile this with sections 6, 7 and 8 of the RFC. List every place the RFC is more expressive than this and propose the cut.

## Topology (owner, 2026-10-08, later the same day)

mote is four parts: mote-cloud (the engine, all state), mote-client (the chat app, Mac and
phone), mote-ui (the renderer) and mote-talk (voice). Your renderer is mote-ui. Two facts shape
it:

- mote-ui holds no state. A Pi may run only mote-ui: it receives surfaces from mote-cloud over the gateway, draws them, sends input events back, and after a restart shows the current surface from its cursor. The Pi stores nothing.
- mote-ui also runs inside mote-client, drawing inline cards in the chat. Asked where a DoorDash order is, a character may draw a map from what the browser session sees, embed a provider widget where one exists, or show a food bag with the minutes underneath. The same renderer must handle a card inside a chat and a full touchscreen surface.

So the targets become: inside the client's chat, a full window on any computer, a Pi
touchscreen, PNG for tests, a terminal as a stretch. Design the display list so one
implementation serves all of them.

## The question to answer

Design and justify the rendering pipeline for mote:

```
language or tree  ->  resolved scene (layout, text, styles)  ->  display list  ->  pixels  ->  presenter
```

Targets, in priority order:

1. A window on macOS (the owner's Mac today).
2. A Raspberry Pi 5 or 4 touchscreen with no desktop environment: DRM/KMS output, libinput or evdev touch, 800x480 to 1280x720.
3. PNG frames for golden-image tests. Every fixture in `docs/rfcs/ui-language.md` section 6 should render to a PNG in CI.
4. A terminal through a pixel graphics protocol (Kitty graphics protocol, iTerm2 inline images, Sixel) as a stretch target, with a cell-based fallback through Ratatui. State clearly what terminals cannot do.
5. "Arbitrary spaces" later: a phone, a web page as a thin blitter of frames, a VS Code webview. Say what the display list would need so these are cheap later.

Properties the pipeline must have:

- Instant. A streamed surface part becomes pixels in the next frame. Set budgets: time from tree part received to first paint, steady frame rate on the Pi, idle CPU and memory on the Pi.
- Animated by design. The model emits whole surfaces. The renderer diffs the old and new surface by id or path and animates the difference: moves, resizes, fades, shared-element transitions, list reorders, springs for touch. The model never writes animation code. Propose the animation model and what the language needs to express intent (an `appear` hint, a `key` for identity, nothing more if possible).
- Pixel-level ownership. Text shaping, fonts, emoji, right-to-left text, hit testing, scrolling physics, focus rings, touch targets, theming. Name the crate or the algorithm for each.
- Accessible. AccessKit or equivalent so VoiceOver on the Mac works.
- One renderer for every target. The same display list goes to the GPU, to a CPU rasterizer, to a PNG and to a terminal protocol. Only the presenter changes.
- Small and testable. The renderer is a crate in `crates/` with golden-image tests, no browser, no JavaScript runtime.

## Candidates to evaluate

Rendering and text: Vello and Parley (Linebender), tiny-skia, cosmic-text, swash, Makepad, egui with wgpu or glow, Iced, Slint (note its license excludes embedded use on the free tier), femtovg, skia-safe, Blitz (Dioxus native, renders markup with Vello, Taffy and Stylo without a browser), Xilem and Masonry, GPUI (Zed), Impeller (Flutter, as a reference design), Rive and dotLottie runtimes for animation ideas.

Layout: Taffy, Morphorm, Clay.

Presenters and input: winit, wgpu, softbuffer, drm-rs, libinput and evdev crates, Kitty graphics protocol, Sixel, Ratatui, notcurses.

For each: maturity, Pi support (Vulkan 1.2 through V3D on Pi 4 and 5, or CPU only), startup time, binary size, text quality, animation story, license, last release, and whether it can be driven from a display list the daemon owns.

## Deliverables

1. `docs/rfcs/renderer.md`. Sections: Key points first (under ten lines); Why; Pipeline design with a diagram in text; Animation model; Candidate table with numbers (measured in a spike where possible, cited otherwise); Target by target (Mac, Pi, PNG, terminal, later spaces); Budgets and how to measure them; What changes in `docs/rfcs/ui-language.md` section 10 (list the proposed edits, do not edit that file); Open decisions with defaults; Acceptance criteria REND-01 to REND-NN, each one sentence plus a **Verify** clause (copy the shape of MOTE-NN in `docs/rfcs/mote.md`).
2. `docs/research/renderer-research.md`. Every source with URL and the facts taken. "Unknown" where nothing was found.
3. A spike is allowed and wanted: under `spikes/renderer/` (not `crates/`), render two fixtures from `docs/rfcs/ui-language.md` section 6 to PNG with the recommended stack and record timings on the Mac. Do not commit. Do not pull models. Do not touch `~/projects/overseer`.
4. `docs/reuse/ui.md`. What mote pins, forks or ports for this concept, with Cargo git dependency snippets pinned to a `rev` or `tag`, following the policy in `docs/reuse/memory.md` if it exists by then.
5. Rows to add to the README board under a new feature "Renderer", written as nested checkboxes in the README's existing shape. Append them to the README under Current features after UI language research. Keep every existing row unchanged.

## Rules

- Plain declarative sentences. One idea per sentence. No em dashes. No emojis. No ticket IDs.
- Say "unknown" instead of guessing. Measured numbers beat quoted numbers; quote the source for the rest.
- Do not install system packages. Cargo dependencies inside the spike are fine.
- Report back with the file paths and a ten-line summary: the recommended stack per target, the animation model, and the three biggest risks.
