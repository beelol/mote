# Vector theme

Vector is the default theme. A surface inherits it when the runtime selects no other theme. Its
identity is a retro red digital barrier on a near-black base. Structure is drawn with thin red
strokes, not filled boxes. Focus and active controls use a brighter red from the same family. When
the resolved layout changes, old strokes break into square pixels and the new borders materialize
from those pixels in one fast transition. The effect suggests that the agent is present behind the
interface without delaying the new interface.

Vector is a general base theme. It renders every element in [../catalog.md](../catalog.md). It adds
no matrix syntax. It fills the theme hook in [../layout-and-theme.md](../layout-and-theme.md): a
model writes no pixel, and the theme owns every value below.

## Token set

Values are the theme's own. matrix source never contains them.

### Color, dark (primary)

| Role | Value | Use |
|---|---|---|
| surface | `#0B0B0C` | The base background. Near black. |
| text | `#E8E3DA` | Primary text. Warm bone. |
| muted | `#8A8578` | Secondary text and captions. |
| line | `#681D24` | Structural strokes: borders, dividers, control outlines. |
| accent | `#F0444C` | Bright red for focus, active controls, carets and selection. |

### Color, light

| Role | Value |
|---|---|
| surface | `#F4F1EA` |
| text | `#1A1A1A` |
| muted | `#6B6659` |
| line | `#9F3538` |
| accent | `#C8322E` |

The runtime follows the system light or dark setting. Dark is the native look.

### Status tones

Tones are the one place more than one hue appears, so meaning stays legible. Each is a thin stroke,
no fill.

| Tone | Value | Meaning |
|---|---|---|
| neutral | `#8A8578` | nothing notable |
| info | `#4AA3C7` | informational |
| success | `#4FB477` | passing, done |
| warning | `#D9A441` | needs attention |
| danger | `#E5484D` | failing, destructive |
| pending | `#6E6A60` dashed | running, in progress |

### Spacing, size, type

| Token | Value |
|---|---|
| spacing unit | 8 |
| container padding | 16 |
| control height | 36, with a 48 hit area for touch |
| radius | 0, sharp vector corners |
| elevation | none, overlays use a scrim plus a stroked outline |
| type family | monospace |
| title | 20 |
| text | 14 |
| caption | 12 |
| code | 13 |

### Icons

Vector draws the fixed semantic names in [../grammar.md](../grammar.md) as thin single weight lines.
Themes provide glyphs for that exact set and cannot extend it.
An unknown name fails before render.

### Motion

| Token | Value |
|---|---|
| duration | 120 ms |
| curve | ease out |

This token specifies the future structural transition in UI-49. The delivered v0 runtime retains
its separate 180 ms standard-control interaction timing and does not implement keyed materialize
animation. The future runtime uses this one duration and curve for every structural state
transition. Hit testing and focus move to the new resolved layout on the first frame. The animation
never delays interaction.
Only borders and structural strokes that were added, removed or moved participate. Text streaming,
caret movement, progress updates and unchanged controls do not restart the effect.

The transition is deterministic for a node identity. The old strokes split into square red pixels.
Those pixels move along short paths into the new stroke geometry, which resolves fully within 120
ms. Repeating the same transition produces the same frames, so golden tests remain stable. Reduced
motion replaces particle travel with an immediate layout swap and a brief red outline pulse.

## How Vector draws each group

Every element is a stroke, not a fill.

- Containers. `screen` is the base surface. `section` and `card` are stroked rectangles with no
  fill. `header` and `footer` are separated by a thin top or bottom rule. `tabs` show a stroked row
  with the active tab marked by a red underline.
- Text. `title`, `text`, and `caption` use the type scale in `text` and `muted`. `code` is accent
  dim monospace. `link` is text with a red underline. `md` renders its subset in the same scale.
- Status. `badge` and `tag` are stroked pills in the tone color, no fill. `banner` is a stroked bar
  with a tone left edge. `stat` is a large mono number with a stroked tone delta arrow. `divider` is
  one thin line.
- Media. `image` and `avatar` show a stroked frame with a placeholder until loaded. `icon` is a
  vector glyph. `timestamp` is muted text.
- Inputs. `field` and `area` are a stroked box with a red caret. `toggle` is a stroked track with a
  red knob. `check` is a stroked box with a red check. `choice` is stroked circles. `select`,
  `multiselect`, and `menu` are a stroked box with a red caret that opens a stroked list.
  `segmented` is a stroked row with the active cell outlined red. `slider` is a thin track stroke
  with a red thumb outline. `stepper`, `date`, and `time` are stroked controls. `button` is a
  stroked outline; `primary` strokes and labels in red; `danger` strokes in the danger tone;
  `disabled` drops to muted.
- Collections. `list` rows are separated by thin rules. `table` is a grid of thin stroked rules with
  a red sort indicator. `grid` tiles are stroked cells. A selected row or tile gets a red outline.
- Feedback. `progress` is a thin stroked track filled by a red stroke, or a rotating stroked arc for
  a spinner. `steps` are stroked nodes joined by a line, the current one red. `typing` is three
  stroked dots that pulse. Skeletons are dim stroked blocks that materialize into content.
- Chat. `message` is a stroked bubble, aligned by role, me in a red edged bubble and them in a line
  edged bubble. `compose` is a stroked field with a red send control.
- Overlays. `dialog` and `sheet` are stroked panels over a dimming scrim. `menu` is a stroked list.
  `tooltip` is a small stroked box. Each draws its strokes on as it appears.

## Status

Vector is the owner's chosen default identity, recorded in [../decisions.md](../decisions.md). The
color values and sizes are a starting set the runtime implements and the owner may tune. The
structure, the stroke over fill rule, and the materialize motion are the fixed identity.
