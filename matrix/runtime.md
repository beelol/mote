# matrix runtime model

matrix is inferred and animated UI riding on an immediate mode GUI. This note records that model so
every recipe can say what you get for free and what animates.

## The three layers again

1. matrix source. The high level language a model writes. Declarative, no pixels, no code.
2. A temporary typed tree. The compile target. It is internal and may change.
3. An immediate mode GUI runtime. It reads the tree and draws the interface every frame.

## Immediate mode under the hood

The runtime is immediate mode, in the sense of egui or Dear ImGui. It does not keep a retained
widget object graph that a model mutates. Each frame it walks the current surface and draws it from
current state. This matches matrix's whole surface replacement: the model emits the next whole
surface, and the runtime simply draws the newest one. There is no patch protocol and no diff a model
has to reason about.

Immediate mode is why matrix can be so small. Layout, hit testing, focus, scrolling, and input are
solved once in the runtime loop, not described per surface. The model names elements; the loop lays
them out and makes them live.

## Inferred

Inferred means the model states intent and the runtime fills in the rest. The model writes
`select "Repo" repo options repos`. The runtime infers the control, the standard height, the label
placement, the keyboard, the open and close behavior, the touch target, and the accessible role.
Every recipe lists this under "Free."

## Animated

Animation lives in the runtime, never in matrix source. Because the runtime redraws every frame, it
can interpolate between the previous surface and the next one instead of cutting. The model supplies
only two things that drive animation, and both already exist for other reasons:

- Node identity. Path and type, plus an optional `key` on a collection. When a keyed row moves
  between two surfaces, the runtime tweens it from its old slot to its new slot rather than
  replacing it.
- State transitions. Loading to loaded, inert to interactive, pending to done, present to absent.
  The runtime owns a standard motion for each: skeletons cross fade to content, a pressed button
  settles, an entering item fades and slides in, a leaving item fades out.

The theme sets the one motion duration and curve, the same way it sets spacing. A model cannot write
a duration, an easing, or a keyframe. There is no animation element and no animation property in the
catalog. Animation is a property of the runtime, like scrolling.

## What this means for a recipe

Each recipe in `recipes/` shows the matrix snippet and then three short lists:

- Slice. The data and bindings the element needs.
- Free. What the immediate mode runtime infers so the model does not write it.
- Animates. What the runtime tweens when this element enters, leaves, or changes.

## The fastest way to build it

The runtime builds a layout on the fly every frame. Each element measures itself independently, a
flexbox pass fits the pieces together, and the pieces draw in parallel. The fastest technology for
that is Rust. Rust gives predictable memory, no garbage collector pauses in the frame loop, data
parallelism without a runtime, and direct GPU access. Use Rust.

The recommended stack, all Rust, all mature, all already cited in the UI language research:

- **Layout: Taffy.** Taffy is a Rust flexbox and grid engine used by Zed, Bevy, and Slint. matrix's
  `row` and `col` map directly onto flexbox, so Taffy is the layout pass with no translation layer.
  It is `no_std` and allocation light, which keeps a per frame layout cheap.
- **Immediate mode shell: egui.** egui is the reference immediate mode GUI in Rust and runs at one
  to two milliseconds per frame. mote already uses eframe, the egui app shell, for mote-client, so
  the runtime shares a base with the client rather than adding a new toolkit. egui owns input, focus,
  and the frame loop.
- **Draw: the GPU through wgpu.** Pieces batch into GPU draw calls and render in parallel on the
  device. wgpu targets Metal on the Mac and Vulkan on Linux, which covers both mote targets. Vello
  is the vector option if crisp scaling matters more than minimal dependencies.
- **Parallel layout and prep: rayon.** Independent subtrees measure and prepare in parallel across
  cores with a work stealing pool, then the flexbox pass composes them. This is the "each piece
  separate, then fit in a flexbox, all drawn in parallel" model made concrete.
- **Accessibility: AccessKit.** AccessKit maps each node to a platform accessibility role, so the
  roles the catalog promises are real on both targets.

The pipeline per frame: parse and validate the newest surface into the temporary tree, measure each
node (parallel, rayon), run the flexbox pass (Taffy), interpolate any in flight animation toward the
new layout, then batch and draw (wgpu). egui drives the loop and input.

This stack is not a new bet. Taffy, egui, AccessKit, and wgpu are the exact tools the UI language
research already evaluated, and eframe is already in the mote client. Choosing Rust here keeps the
runtime consistent with the rest of mote while giving the fastest frame loop available.

## Targets and a second consumer

The runtime must run on the Mac, on a Raspberry Pi touchscreen, and on an iPad. The Rust stack
covers all three. wgpu renders through Metal on the Mac and the iPad and through Vulkan on the Pi,
and egui with winit builds as an iOS app, so one Rust runtime is the Mac client, the Pi renderer,
and the iPad app. Keep going with Rust.

A consumer is anything that renders a validated surface. The temporary tree is the consumer
contract, which is exactly why a single wire format matters: it lets more than one consumer exist
without changing matrix. The primary consumer is the Rust runtime. A second consumer is sanctioned
if the Rust path is ever impractical on iOS: a native immediate mode consumer, such as SwiftUI,
that renders the same validated tree and sends the same events back. Any second consumer must be
hyper fast and immediate mode, must render every element in the catalog, and must not change the
language or the tree. matrix and the daemon stay identical across consumers.

## How the host runs it

The runtime is a consumer process, not code inside the daemon. In mote, moted stays headless under
launchd and owns data, binding subscriptions, validation, and surface history. It streams validated
surface tree commits and binding values over the existing IPC event stream and receives ui.event
back. The runtime is the client on the Mac and the iPad and runs standalone on the Pi. It holds only
interaction state and redraws from the newest committed surface each frame. This matches the v0
design where mote-client connects to the socket, renders a surface through mote-ui, and sends events
back, and it is the path to replace the dead eframe client. The first milestone is a minimal runtime
that reads one validated tree from the stream and draws it.

## Interim v0 consumer contract

The first milestone consumes `crates/mote-ui::Node` unchanged.
The node variants, allowed keys and limits in `crates/mote-ui/src/lib.rs` are authoritative.
The wire event remains `{"event": name, "args": {...}}` from v0 section 12.
This grammar freeze changes no production code, schema, fixture or IPC method.
A consumer can draw existing validated JSON before a Matrix parser is authorized.

| Matrix subset | Existing v0 node | Lowering rule |
|---|---|---|
| `text` | `text` | Resolve the literal or Value into `value`. |
| `col` | `stack` | Lower children in reading order. |
| Default `row` | `row` | Lower children in reading order. |
| `list` without keys, selection or overrides | `list` | Expand the item template against a bounded host snapshot. |
| Plain `table` | `table` | Resolve labels into `columns` and scalar projections into string `rows`. |
| Default text `field` | `field` | Resolve label, Field path and initial string value. |
| `toggle` | `toggle` | Resolve label, Field path and initial boolean value. |
| `select` | `select` | Resolve label, Field path, string options and initial value. |
| Plain zero argument `button` | `action` | Copy its label and published Action name into `event`. |
| Determinate unlabeled `progress` | `progress` | Resolve its number into `value`. |

An untitled screen lowers to a stack of its children.
A singleton untitled screen wrapper and a singleton item template wrapper are elided.
An explicit `col` always retains its stack wrapper.
All references resolve on the host before the JSON is validated and committed.
The consumer never receives source paths masquerading as text content.
Bounds apply after collection expansion as well as during source validation.

The remaining catalog has no lossless v0 representation.
Named layout roles, headers, footers, semantic text roles, keys, selection, overlays, message roles,
asset media, nondefault field kinds and action arguments cannot be added as arbitrary JSON keys.
The `chat` v0 node accepts arbitrary children but does not encode Matrix message roles or bodies.
The existing chat fixture is therefore not an exact Matrix chat equivalent.
No unsupported node is silently flattened into text or a stack.
The host rejects unsupported lowering with `E_LOWER_UNSUPPORTED` before committing the candidate.
Existing JSON consumers continue to use the v0 rejection diagnostic behavior.

The first runtime task renders all eleven existing node variants and existing fixtures.
It does not implement the entire Matrix catalog through an invented wire extension.
After `UI-00`, broader Matrix lowering needs a separately approved consumer contract revision.
That revision must carry keys, selection and typed interaction metadata before claiming those behaviors.
The owner confirmation list records this boundary.

## Status

This is a recommended architecture from owner direction ("inferred and animated ui on immediate gui
under the hood," and build the interpreter in the fastest technology, Rust if it is fastest). It is
recorded as a decision in `decisions.md` and is open to owner revision. The stack names are
recommendations backed by the research, not a final commitment to specific crates.
