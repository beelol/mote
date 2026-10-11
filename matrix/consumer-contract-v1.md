# Matrix consumer contract version 1

Version 1 is the immutable first Matrix consumer contract. It carries slider, anchored screen
layout and fill plus the v0-equivalent elements required to compose that surface. It is separate
from the frozen v0 tree and from the host binding manifest version.

The daemon emits this contract for the frozen slice after the version 1 compiler, client renderer,
accessibility tree, layout checks and event admission land together. Valid sources outside this
slice retain their lossless v0 emission. A supporting client prefers version 1 and sends its explicit
contract context. Unsupported clients display `surface`.

## Event payload

Version 1 keeps the existing `ui.surface` event kind. One event carries both contracts.

```json
{
  "surface_id": "matrix",
  "surface": {
    "type": "text",
    "value": "This Matrix surface requires a newer Mote client."
  },
  "matrix_surface": {
    "version": 1,
    "root": {
      "type": "screen",
      "children": []
    }
  }
}
```

`surface` is mandatory and must validate as the unchanged v0 tree. `matrix_surface` is mandatory
when version 1 is emitted and must validate against `schemas/tree.matrix.v1.json`. The names use
snake case exactly. Both values belong to one event sequence and advance one durable cursor.

The producer uses a faithful v0 lowering only when the complete Matrix source is lossless in v0.
If any declaration or semantic property is not lossless, the producer uses the exact noninteractive
v0 Text fallback shown above. It does not flatten anchors, fill, slider or later elements into an
approximate v0 tree.

A legacy client reads `surface` and ignores `matrix_surface`. A version-aware client validates the
declared Matrix contract before using it. A missing, malformed, unknown or unsupported Matrix
contract falls back to the valid v0 `surface`. It does not erase the previously committed surface
when both representations are invalid.

## Interaction context

The stored `ui.event` body remains `{"event": name, "args": {...}}`. The RPC context adds an
optional integer `surface_contract`. Omission means v0. Version 1 interaction sends
`surface_contract: 1`. The daemon binds the contract number and `surface_seq` to a control table
compiled for that exact representation. It does not assume the v0 fallback path matches the Matrix
path.

## Version 1 surface

The surface object has exactly two keys.

| Key | Type | Rule |
|---|---|---|
| `version` | integer | Exactly `1`. |
| `root` | node | Exactly one `screen`. |

Unknown keys and unknown node types reject the whole Matrix surface. The maximum compact encoded
size is 256 KiB. The root counts as depth one. Maximum depth is 12 and maximum node count is 2,000.
All strings are inert Unicode text. No property carries markup or presentation measurements.

## Frozen node slice

Every node is an object with `type` first in canonical serialization. `children` is last when it is
present. Optional values are omitted when they hold their default.

| Type | Keys after `type` | Contract |
|---|---|---|
| `screen` | optional `title`, `children` | Root only. At most one direct `header` and one direct `footer`. |
| `col` | optional `fill`, `alignment`, `distribution`, `children` | Vertical layout. |
| `row` | optional `fill`, `wrapping`, `alignment`, `distribution`, `children` | Horizontal layout. Defaults are wrap, start and packed. |
| `header` | `children` | Direct screen child only. |
| `footer` | `children` | Direct screen child only. |
| `text` | `value`, optional `fill` | Literal resolved text. |
| `toggle` | `label`, `field`, `value`, optional `fill` | Boolean field control. |
| `select` | `label`, `field`, `options`, `value`, optional `fill` | Options are nonempty unique strings and contain `value`. |
| `slider` | `label`, `field`, `value`, `min`, `max`, optional `step`, optional `fill` | Finite JSON numbers. `min <= value <= max`. Step is positive and value is aligned from min. |
| `button` | `label`, `event`, optional `intent`, `confirm`, `disabled`, `fill` | Zero-argument action only in version 1. Boolean options default to false. Intent is `primary` or `danger`. |

`alignment` is `start`, `center` or `end`. `distribution` is `between` or `around`.
`wrapping` is `wrap` or `nowrap`. The absence of `wrapping` means wrap. The absence of alignment
means start. The absence of distribution means packed.

`fill` is valid only on a direct child of `row` or `col`. A filled child takes the remaining main
axis space. Fill never changes the cross axis. Header and footer stay outside the scrolling screen
body. The body owns the remaining height and scrolls automatically.

Version 1 has no model-authored node id. Runtime identity is node type and structural path. This
version has no repeated keyed collection, so it needs no row key representation.

The exact canonical example is `crates/mote-ui/fixtures-v1/01_slider_layout.json`.

## Closed validation

The Rust type and strict validator are authoritative together. Serde rejects missing required
values and type mismatches. The validator rejects unknown keys before decoding, enforces resource
limits, checks root and anchor placement, checks fill placement, checks nonempty control names and
checks select and slider invariants. A failure rejects the whole Matrix surface. It never produces
a partial version 1 tree.

The schema is generated from the Rust type by
`cargo run -p mote-ui --example gen_matrix_v1_schema`. The committed result is
`schemas/tree.matrix.v1.json`. Generation is deterministic and a test fails on drift.

## Later contract versions

Version 1 does not freeze the unresolved catalog. Later immutable versions are cumulative
supersets, so a later surface can combine version 1 nodes with newly frozen families. A producer
emits a later version only after its complete semantics and consumer support are frozen.

The following Matrix catalog words remain unresolved in version 1:

- Containers: `section`, `card`, `tabs`, `tab`.
- Text and media: `title`, `caption`, `label`, `code`, `md`, `link`, `badge`, `tag`, `stat`, `kv`,
  `icon`, `image`, `avatar`, `timestamp`, `divider`.
- Inputs and forms: `field`, `area`, `check`, `choice`, `multiselect`, `segmented`, `stepper`,
  `date`, `time`, `search`, `form`, and action arguments.
- Collections: `list`, `item`, `table`, `column`, `grid`, `tile`, keys and selection.
- Feedback and chat: `progress`, `steps`, `status`, `banner`, `empty`, `error`, `typing`, `chat`,
  `message`, `compose`.
- Overlays: `dialog`, `sheet`, `menu`, `tooltip`.

Before those words enter a later contract, the design must freeze their wire representation. The
open decisions include the constrained Markdown subset, asset fetch and cache policy, status value
to tone mapping, typed action arguments and pending state, explicit pending and failed binding
states, stable row keys, selection updates, overlay focus and contextual built-in actions. Version
1 invents none of those representations.
