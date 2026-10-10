# matrix layout and theme

matrix has a tiny layout model and exactly one theme per surface. This is on purpose. A model should
name a shape, not design one. Everything about size, spacing, and color is standard and inferred.

## Layout vocabulary

There are two containers and two anchors.

- `col` lays its children top to bottom.
- `row` lays its children left to right, and wraps to the next line when the surface is narrow.
- `header` is an anchored region at the top of a screen. It does not scroll with the body.
- `footer` is an anchored region at the bottom of a screen. It is where primary actions live.

That is the whole model. There is no grid with custom tracks, no absolute position, no z-index, and
no manual gaps. The one wrapping grid of equal cells is the `grid` element in the catalog, which is a
collection, not a layout primitive.

### Named roles, never numbers

A child may carry a small named role that affects how it sits in its parent. A role is a word, never
a number.

| Role | On | Effect |
|---|---|---|
| `fill` | a child of `row` or `col` | Take the remaining space along the main axis. |
| `wrap` | `row` | Allow wrapping. This is the default and rarely written. |
| `nowrap` | `row` | Keep on one line and let the row scroll sideways. |
| `start`, `center`, `end` | `row` or `col` | Align children on the cross axis. Default is `start`. |
| `between`, `around` | `row` or `col` | Distribute space between children. Default is packed. |
| `secondary` | any block | Render in the quieter secondary density, for sub text. |

A model reaches for a role only when the default is wrong. Most surfaces write none of these.

## Scrolling is automatic

Scrolling is never written. Any container taller than the surface scrolls. A `list`, `table`,
`grid`, and `chat` scroll inside their own box. A `header` and `footer` stay put while the body
between them scrolls. Scroll position survives a re render of the same node, so a streamed update
does not jump the view. The only control a model has is `nowrap` on a `row`, which chooses sideways
scrolling over wrapping for that one row.

## Heights and spacing are standard

Every standard size comes from the theme, not from the model.

- Control height is one standard value. Every `field`, `select`, `toggle`, `button`, `slider`, and
  the rest share it, so a form lines up without any effort.
- Spacing between children comes from one spacing scale. A `col` and a `row` apply the standard gap.
- Padding inside a `card`, `section`, `dialog`, and `screen` comes from one padding scale.
- Text sizes come from one type scale tied to the element: `title`, `text`, `caption`, `code`.

A model cannot set any of these. There is no pixel value anywhere in matrix. This is what lets an
agent generate a correct, aligned interface without reasoning about measurements.

## The theme

A surface inherits exactly one theme. The theme is a token set. It defines:

- The spacing scale, the padding scale, and the one control height.
- The type scale for each text element.
- The corner radius and the one elevation used for cards and overlays.
- The color roles for light and dark: surface, text, muted text, line, and the accent.
- The status tones: `neutral`, `info`, `success`, `warning`, `danger`, `pending`.
- The icon set the `icon` element draws from.

The theme is chosen by the runtime, not by the model. A child node cannot select a theme and cannot
override a token. There are no per element colors, no per element fonts, and no per element
paddings in matrix source.

### The default theme

matrix ships with one default theme so a surface always has somewhere to inherit from. The default
theme is Vector: thin retro red on a dark base, an Atari vector line look, with structure drawn as
thin strokes and one red accent. It follows the system light or dark setting, its control height is
comfortable for both pointer and 48 pixel touch, and its status tones are the conventional six. A
runtime that defines no theme gets Vector, so a model can emit a surface the first day with zero
theme setup. The full token set is in [themes/vector.md](themes/vector.md).

### Theme overrides padding and the standard sizes

The theme, and only the theme, may override the standard values. A different theme may set a tighter
padding scale, a denser spacing scale, a shorter control height, a different radius, or a different
palette. Because every element reads these from the theme, one theme swap re spaces and re colors the
whole interface at once, and no matrix source changes. This is the intended override path: the owner
ships or edits a theme, the model keeps writing the same matrix, and the look follows the theme.

## Why this is enough

The catalog in [catalog.md](catalog.md) already carries the structure of a surface. Layout only has
to place those elements in reading order and let the runtime handle the rest. A single column of
sections, with rows where a few things sit side by side, and anchored top and bottom regions, covers
the overwhelming majority of generated surfaces. The cases that look like they need more are usually
a `list`, a `table`, or a `grid`, which are collections, not layout. See
[examples.md](examples.md).
