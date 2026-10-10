# matrix grammar

This is the frozen design grammar for the current catalog.
It is a proposal for owner confirmation at `UI-00`.
It does not authorize a parser or change the v0 build contract.
The snippets in [examples.md](examples.md) and [recipes/](recipes/README.md) use this grammar.

## Declaration shape

A complete surface is exactly one `screen` declaration.
Recipes may show a fragment that is inserted into a screen.
A fragment has the same declaration grammar and requires the bindings and enclosing scope its slice names.
Indentation is for reading.
Whitespace separates tokens and has no other meaning.
Newlines do not terminate a declaration.
A declaration ends after its permitted arguments or its closing block.
An element word begins the next declaration.
An element word in an argument position is an identifier rather than a new declaration.

```text
surface     = screen-declaration end-of-input
block       = "{" declaration* "}"
identifier  = ASCII letter or "_", followed by ASCII letters, digits or "_"
path        = identifier ("." identifier)*
reference   = path | "." path
string      = JSON string
number      = JSON number
options     = "[" string ("," string)* "]"
action      = "->" identifier ["{" argument ("," argument)* "}"]
argument    = identifier [":" reference]
```

An argument without `:` is shorthand for a reference with the same name.
An action payload must contain at least one argument if braces are present.
No action parentheses or trailing commas are accepted.
Only string option literals and numeric constraint literals are accepted.
The empty string is accepted as display text but not as an option, stage, icon name or required label.
Booleans are field values supplied by the host and are not source literals.
JSON string escapes are accepted, including escaped quotes and Unicode escapes.
Raw newlines in strings, invalid escapes, nonfinite numbers and leading zero numbers are rejected.
Comments, semicolons and implicit string interpolation are absent.
`#`, `//` and `/*` outside strings are errors.
Names and words are case sensitive.
UTF-8 source and decoded strings must be valid Unicode.

## Closed declaration signatures

The notation below describes signatures and is not matrix source.
Square brackets mark an optional part.
`display` means a string literal or a typed read reference.
Display references accept strings, numbers and closed enums.
The host publishes the canonical display format for each type.
`read` means a `Value<T>` reference or a projection from its row.
`field` means a `Field<T>` path.
Each optional part appears at most once and in the order shown.
No other property or modifier is accepted.

| Elements | Signature after the element word |
|---|---|
| `screen`, `section` | `[string] block` |
| `card` | `[string] [collapsible] block` |
| `col` | `[alignment] [distribution] block` |
| `row` | `[wrap or nowrap] [alignment] [distribution] block` |
| `tabs`, `header`, `footer` | `block` |
| `tab` | `string block` |
| `title`, `text`, `caption`, `code`, `badge`, `tag`, `status` | `display`; only `status` may append `live` |
| `label` | `string` |
| `md` | `read` of string type |
| `link` | `display to read` of URL type |
| `stat` | `string read [delta read]` |
| `kv` | `string read` |
| `icon` | `string` from the fixed icon set |
| `image`, `avatar` | `read alt display`; source has asset type and alt has string type |
| `timestamp` | `read [relative]` of datetime type |
| `divider`, `typing` | no arguments |
| `field` | `string field [kind kind-word] [min number] [max number] [pattern string]` |
| `area`, `toggle`, `check`, `time`, `search` | `string field` |
| `choice`, `select`, `multiselect` | `string field options (options or read)` |
| `segmented` | `string field options options` |
| `slider` | `string field min number max number [step number]` |
| `stepper` | `string field [min number] [max number] [step number]` |
| `date` | `string field [min read] [max read]` of date type |
| `button` | `string [primary or danger] [confirm] [disabled] action` |
| `form` | `[string] block` |
| `list`, `table`, `grid` | `from read [key path] [select one or select many] block` |
| `item`, `tile` | `block` |
| `column` | `string .path [kind column-kind]` |
| `progress` | `[value read] [label string]` of number type |
| `steps` | `stages options current read` of string or matching closed enum type |
| `banner` | `display tone (info or warn or error) [action]` |
| `empty` | `string` |
| `error` | `string [-> Retry]` |
| `chat` | `from read [key path] block` |
| `message` | `role (me or them or system or read) body read [block]` |
| `compose` | `field send action` |
| `dialog`, `sheet` | `string block` |
| `menu` | `trigger (string or icon string) block` |
| `tooltip` | `string block` |

Alignment is one of `start`, `center`, `end`.
Distribution is one of `between`, `around`.
Default alignment is `start` and default distribution is packed.
`row` wraps by default.
The `fill` role may appear immediately before a declaration's action or block, or at its end if it has neither.
It is allowed only on a direct child of `row` or `col`.
A `col` with both alignment and fill is written `col end fill { ... }`.
`fill` appears at most once.
No `secondary` modifier exists because it changes per element presentation.
Use the catalog's `caption` element for secondary text.
The field kind words are `text`, `email`, `url`, `number`, `phone`, `password`.
The default field kind is `text`.
Column kind words are `text`, `number`, `money`, `date`, `status`.
The host type supplies the default column kind.
`min` and `max` on text fields count Unicode scalar values.
Those bounds must be nonnegative integers.
They bound numeric values on number fields.
`step` must be positive and `min` cannot exceed `max`.
`pattern` uses the Rust regex language without lookaround or backreferences.
The host rejects an invalid pattern before rendering.
Passwords have string type and are redacted before any event storage.
Progress values are finite numbers in the closed interval from zero to one.
Stat and delta references accept strings or numbers.
Key value references accept strings, numbers or closed enums.
Column kinds must match the host's declared projection type and formatter.
Message role references have exactly the closed enum me, them, system and body references have string type.
Search, area and compose fields have string type.
Toggle and check fields have boolean type.
Slider and stepper fields have number type.
Time fields have time type and date fields have date type.
Options are nonempty and unique and must match the field's closed enum type.
Stages are nonempty and unique and the current value must name one stage.

## Nesting and scopes

Only the declarations whose signature contains `block` accept children.
A `screen` cannot be nested.
At most one `header` and one `footer` are allowed and each must be a direct screen child.
A `tabs` block contains one or more `tab` declarations and nothing else.
A `tab` is valid only inside `tabs`.
A `list` contains exactly one `item` template and optional `empty` and `error` declarations after it.
A `grid` has the same rule with exactly one `tile` template.
A `table` contains one or more `column` declarations and optional `empty` and `error` declarations after them.
A `chat` contains exactly one `message` template and optional `empty` and `error` declarations after it.
Each collection accepts at most one `empty` and one `error` in that order.
`item`, `tile`, `column` and `message` are valid only in their matching collection.
An `empty` or `error` override is valid only in a collection.
Each collection starts a new row scope for its template or columns.
A nested collection replaces that row scope inside its own template.
Relative `.path` references resolve in the nearest row scope.
There is no implicit root record and no reference to an outer row.
Ordinary container children follow the reading order in source.
A `menu` contains one or more `button` declarations and nothing else.
A `tooltip` contains exactly one ordinary declaration.
An ordinary container may contain any declaration except those restricted above.
An overlay is an ordinary screen descendant and inherits lexical bindings.
The source declaration makes it present until the runtime dismisses it or the host replaces the surface.
A form contains controls and exactly one `primary` button as its submit.
Nested forms are rejected.
Enter activates that submit when its fields are valid.
Other buttons inside the form are ordinary actions.
There is no second form submit annotation.

## Bindings and host types

The host publishes a versioned manifest before source validation.
It names Value paths, Field paths, action names and built in action contracts.
It owns record types, nested fields, enum members, display formats and initial field values.
Source cannot declare or infer a record type from untrusted text.
The `PullRequest` record in the worked example is host documentation, not source.
Every read reference must resolve to a manifest Value or a row projection of one.
Every control path must resolve to a manifest Field of its required type.
Two controls cannot own the same Field in one lexical instance.
Fields owned outside repeated templates belong to the whole surface instance.
Their actions may read them regardless of declaration order or intervening containers.
Fields owned inside a repeated template belong to that row instance only.
Their values and control identity are namespaced by the collection row identity.
Template fields cannot be referenced from outside that template instance.
The manifest must not use `selected` as a root Value or Field name.
An action may read Field paths, Value paths and in-scope row or selection projections.
There are exactly three binding kinds: Value, Field and Action.
Unknown paths, types, enum members and actions reject the whole candidate surface.
Helper calls such as `count.of(...)`, `url.of(...)` and arithmetic are absent.
The host publishes computed values such as `github.pull_request_count` instead.

Selection is optional on `list`, `table` and `grid` only.
It publishes `selected` in the collection's immediate parent block.
Descendant ordinary blocks such as a footer can read that selection.
The parent block may contain at most one selectable collection.
A selection inside an item, tile or message never escapes that template instance.
`select one` has type `Option<T>` and `select many` has type `List<T>`.
`selected.path` is allowed only for `select one`.
The runtime disables an action needing a single selection until one row is selected.
There is no projection across a selected list.
Pass `selected` to an action whose declared input accepts `List<T>`.
Use explicit host actions for bulk operations on those rows.

## Actions and events

Only `button`, `banner`, `error` and `compose` accept an action.
There is no bare arrow on an item and no `send` clause on search.
Use a button for search submission or opening a row.
All action arguments are named references.
Names must be unique and match the published Action contract exactly.
Missing required arguments and extra arguments fail closed.
The host checks the derived source argument types against that contract.
Zero argument actions omit their payload braces.
Built ins are `Open{url: reference}`, `Copy{text: reference}`, `Back`, `Dismiss`, `Retry`, `Cancel`.
`Open` accepts a host validated URL rather than a raw filesystem path.
`Retry` inside an error override targets the enclosing failed collection binding.
Elsewhere it targets the surface's failed binding set.
`Dismiss` targets the nearest overlay and is rejected outside an overlay.
`Back`, `Retry` and `Cancel` are inert when their host context has no applicable target.
Model actions must exist in the manifest and cannot shadow built ins.
Events retain the v0 wire shape `{"event": name, "args": {...}}`.
Node identity and surface revision belong to host validation context.
They are not extra keys on a v0 tree node or an invented wire envelope.
An event never contains source references.
It contains the validated values read at activation.

## Identity and rejection

The fixed icon names are `add`, `back`, `bell`, `calendar`, `check`, `clock`, `close`, `copy`,
`delete`, `down`, `edit`, `error`, `file`, `folder`, `image`, `info`, `left`, `mail`, `more`,
`open`, `right`, `search`, `settings`, `up`, `user`, `warning`.
Themes draw those names and cannot add or rename them.
Runtime inferred status icons use the same set.
An unknown icon name fails closed.

A key names one row field path on `list`, `table`, `grid` or `chat`.
The field must be a stable string or integer supplied by the host.
Its value must be unique and nonnull within that collection snapshot.
Duplicate or invalid keys reject the update.
Unkeyed identity is the node's type and structural path.
Keyed row identity is the enclosing collection identity and typed key value.
Reconciliation does not merge identical keys from separate collections.
Keys alone are sufficient for reorder motion.
No animation hint, floating action primitive, tracked grid, absolute layout or z-index is added.
The runtime preserves matching state across whole surface replacement.
Any syntax, scope, type or lowering failure leaves the previous valid surface intact.
The v0 diagnostic text node remains the existing consumer behavior for rejected JSON.
It is not permission to render part of an invalid Matrix candidate.
Limits apply to both source declarations and the resolved v0 tree: depth 12 and 2,000 nodes.
Source UTF-8 and compact resolved JSON are each limited to 256 KiB.
The host checks those bounds during source reading and binding expansion before further allocation.
Unknown element words, modifier words, keys, bindings, duplicate properties and trailing tokens fail closed.
The consumer receives only the unchanged v0 tree described in [runtime.md](runtime.md).
