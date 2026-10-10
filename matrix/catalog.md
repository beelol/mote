# matrix catalog

The catalog is the closed list of elements matrix understands. A model may only use words from this
list. Adding a word is a versioned change to the language, never something a model does. The list is
chosen to express the common surfaces an agent generates when it turns an intent into an interface.

Each element has a slice. The slice is the small set of data and bindings the element needs to be
useful. The slice is the whole contract. An element asks for nothing beyond its slice, and a model
supplies nothing beyond its slice. This is what keeps a matrix program short.

Bindings in the slice column use the three typed forms from
[bindings-and-state.md](bindings-and-state.md): `Value<T>` is read only data, `Field<T>` is an
editable value, and `Action` is an event the runtime accepts. A literal in quotes is a fixed string
the model writes directly.

No element takes a size, a color, a font, a margin, or a padding. Heights and spacing are standard
and come from the theme. See [layout-and-theme.md](layout-and-theme.md).

## Containers

A container groups children. It carries no content of its own.

| Element | What it is | Slice | Children | Role |
|---|---|---|---|---|
| `screen` | A full surface. Exactly one is the root. | title: literal, optional | any | main region |
| `section` | A titled group inside a screen. | title: literal, optional | any | section |
| `card` | A visually grouped block. Optional collapse. | title: literal, optional; `collapsible` | any | group |
| `col` | Vertical flex stack. The default for a screen body. | none | any | none |
| `row` | Horizontal flex line. Wraps when narrow. | none | any | none |
| `tabs` | A set of labeled panels, one visible at a time. | none | `tab` only | tablist |
| `tab` | One labeled panel inside `tabs`. | title: literal | any | tab, tabpanel |
| `header` | Anchored top region. Stays while the body scrolls. | none | any | banner |
| `footer` | Anchored bottom region for primary actions. | none | any | contentinfo |

`col` and `row` are the whole layout model, plus the two anchors `header` and `footer`. There is no
grid and no absolute position. See the layout document for wrap, alignment, and growth.

## Text and value display

| Element | What it is | Slice | Role |
|---|---|---|---|
| `title` | The name of a screen or section. | text: display operand | heading |
| `text` | Plain body text. Wraps. Never markup. | text: display operand | text |
| `caption` | Secondary, smaller, quieter text. | text: display operand | text |
| `label` | The name of an adjacent control or value. | text: literal | label |
| `code` | Monospace inline text for ids and snippets. | text: display operand | code |
| `md` | A constrained Markdown subset. No raw HTML, no images. | text: `Value<str>` | document |
| `link` | Text that opens a resource through the runtime. | text: display operand; to: `Value<url>` | link |
| `badge` | A short status word with a theme status tone. | text: display operand | status |
| `tag` | A short neutral keyword chip. | text: display operand | text |
| `stat` | A large number with a label and optional delta. | label: literal; value: `Value<str or number>`; delta: same type choices, optional | group |
| `kv` | A label and its value on one line. | label: literal; value: typed string, number or closed enum read | group |
| `icon` | A theme glyph for a fixed semantic name. | name: literal from the fixed grammar set | img |
| `image` | A picture the runtime fetches and caches. | src: `Value<asset>`; alt: literal or `Value<str>`, required | img |
| `avatar` | A small round image for a person or repo. | src: `Value<asset>`; alt: literal or `Value<str>`, required | img |
| `timestamp` | A time, formatted and optionally relative. | value: `Value<datetime>`; `relative` | time |
| `divider` | A thin rule between blocks. | none | separator |

`badge` tone is chosen by the runtime from the value against a small closed set of status meanings,
not by the model writing a color. See the status tones note below.

## Input

Every input binds to a `Field<T>`. The field type decides the control behavior and the keyboard. A
field that is part of a `form` contributes to that form's action input. Standard control height
comes from the theme.

| Element | What it is | Slice | Field type |
|---|---|---|---|
| `field` | A single line text entry. Kind sets the keyboard. | label: literal; bind: `Field<T>`; kind: one of text, email, url, number, phone, password; min, max, pattern optional | str or number |
| `area` | A multi line text entry. | label: literal; bind: `Field<str>` | str |
| `toggle` | An on or off switch. | label: literal; bind: `Field<bool>` | bool |
| `check` | A checkbox, on or off, inline with a label. | label: literal; bind: `Field<bool>` | bool |
| `choice` | A radio group. One value from a closed set. | label: literal; bind: `Field<T>`; options: `Value<List<T>>` or literal set | closed enum |
| `select` | A pull down menu. One value from a closed set. | label: literal; bind: `Field<T>`; options: `Value<List<T>>` or literal set | closed enum |
| `multiselect` | A pull down that accepts many values. | label: literal; bind: `Field<List<T>>`; options: as select | list of enum |
| `segmented` | A small inline switch between a few options. | label: literal; bind: `Field<T>`; options: literal set | closed enum |
| `slider` | A value dragged along a range. | label: literal; bind: `Field<number>`; min; max; step optional | number |
| `stepper` | A number nudged by plus and minus. | label: literal; bind: `Field<number>`; min, max, step optional | number |
| `date` | A date picker. | label: literal; bind: `Field<date>`; min, max optional | date |
| `time` | A time picker. | label: literal; bind: `Field<time>` | time |
| `search` | A text field that filters a nearby collection. | placeholder: literal; bind: `Field<str>` | str |
| `button` | A control that sends an action. | text: literal; `-> Action{args}`; `primary` or `danger`, `confirm`, `disabled` optional | none |
| `form` | A group of fields with one submit action. | title: literal optional; exactly one primary button names the submit action | input fields and buttons |

A `select` is the pull down the owner asked for. A `slider` is the range control. `choice` and
`segmented` are the same closed value idea at different sizes. The options of `select`, `choice`,
`segmented`, and `multiselect` must share the field's closed value type, and the runtime rejects an
option that is not in that type before rendering.

## Collections

A collection renders many items from one typed list binding. The model writes one item template. The
runtime repeats it per row. A model never unrolls a list by hand.

| Element | What it is | Slice | Children | Role |
|---|---|---|---|---|
| `list` | A vertical list of items. Scrolls. | from: `Value<List<T>>`; `select one` or `select many` optional; key optional | one `item` | list |
| `item` | The template for one list row. Projects fields with `.field`. | the row type T is in scope | any | listitem |
| `table` | Rows and typed columns. Sortable by column type. | from: `Value<List<T>>`; key and `select one` or `select many` optional | `column` set | table |
| `column` | One typed column of a table. | label: literal; value: `.field`; kind: text, number, money, date, status optional | none | columnheader |
| `grid` | A wrapping set of equal cards. For tiles and galleries. | from: `Value<List<T>>`; key and selection optional | one `tile` | list |
| `tile` | The template for one grid cell. | the row type T is in scope | any | listitem |

`list` and `grid` differ only in flow. `list` stacks, `grid` wraps into even cells. A table is for
data the person compares across columns. Selection on a collection yields a `selected` value that an
action can read. See the selection rule in the bindings document.

## Feedback and state

These express progress and the absence of data. Most are supplied by the runtime without the model
writing them, but the model may write an explicit one to override the default message.

| Element | What it is | Slice | Role |
|---|---|---|---|
| `progress` | A determinate bar or an indeterminate spinner. | value: `Value<number>` optional; label: literal optional | progressbar |
| `steps` | A sequence of stages with a current one. | stages: literal set; current: `Value<str>` | list |
| `status` | A single inline status line, optionally live. | text: display operand; `live` optional | status |
| `banner` | A prominent message with a tone and optional action. | text: display operand; tone: info, warn, error; `-> Action` optional | status |
| `empty` | Shown by a collection with zero rows. Overrides the default. | text: literal | status |
| `error` | Shown when a bound value or action fails. Overrides default. | text: literal; `-> Retry` optional | alert |
| `typing` | A chat typing indicator. | none | status |

A collection shows a standard empty message on its own. A bound value that fails shows a standard
error with a retry on its own. Writing `empty` or `error` only changes the words, never whether the
state exists.

## Chat

| Element | What it is | Slice | Children | Role |
|---|---|---|---|---|
| `chat` | A scrolling transcript of messages. | from: `Value<List<Message>>`; key optional | one `message` | log |
| `message` | One message. Role sets alignment and tone. | role: me, them, system or a typed value of that enum; body: `Value<str>` projection | any | listitem |
| `compose` | A text entry and send action for a chat. | bind: `Field<str>`; send: `-> Action{text}` | none | none |

## Overlays

An overlay floats above the current screen. The runtime owns dismissal, focus trapping, and the
scrim. A model declares intent, not behavior.

| Element | What it is | Slice | Children | Role |
|---|---|---|---|---|
| `dialog` | A modal that interrupts for a decision. | title: literal | any, usually a `form` or buttons | dialog |
| `sheet` | A panel that slides from an edge for a side task. | title: literal | any | dialog |
| `menu` | A pull down list of actions from a trigger. | trigger: literal or `icon` | `button` set | menu |
| `tooltip` | Hover or long press help on its one child. | text: literal | exactly one element | tooltip |

## Status tones

Several elements show a status tone rather than a color: `badge`, `banner`, and the `status` column
kind. The model supplies a value. The runtime maps that value to one of a small closed set of
meanings: `neutral`, `info`, `success`, `warning`, `danger`, `pending`. The theme assigns the actual
color to each meaning. The model never writes a color and never writes the meaning directly for a
badge whose value already implies it, such as a pull request state.

## What is deliberately absent

matrix has no element for a free canvas, a web view, an embedded script, a custom widget, a video
player, or a rich text editor. It has no grid with arbitrary track sizing. It has no element whose
only purpose is spacing or decoration. If a surface seems to need one of these, that is a signal to
reconsider the surface, not to extend the catalog. The deferred list and the reasons are in
[decisions.md](decisions.md).

## Exact source contract

[grammar.md](grammar.md) defines the closed signatures, child restrictions and scopes.
Its display operands accept a literal or a typed string, number or closed enum value.
This applies to `badge`, `tag`, `status`, `banner` and link text as well as ordinary text.
Image and avatar alt text accepts a literal or a typed string value.
Action payloads use `-> Action{name: reference}` with shorthand `-> Action{name}`.
Buttons may carry `primary` or `danger` intent followed by `confirm` and `disabled`.
Only collections accept `key` and selection.
Only `timestamp` accepts `relative`.
Collection empty and error overrides are children after the template or columns.
No slice implies an expression, implicit row record or additional modifier.
