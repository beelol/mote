# matrix bindings and state

matrix separates meaning from data. The model writes the shape of a surface and names the data it
wants. The runtime owns the data and the wiring. This document defines the three typed bindings, who
owns which state, how an event travels, and how a streamed surface renders.

## Three typed bindings

Every connection between a surface and the world is one of three typed forms. There are no magic
strings and no untyped references. Each binding is checked before the surface renders, and a binding
that does not type check never reaches a screen.

### Value<T>

`Value<T>` is read only data the runtime supplies. A `text`, `stat`, `list`, or `table` reads a
value. The runtime resolves it, keeps it current, and shows a loading state until it arrives and an
error state if it fails. The model never fetches and never caches.

```
text github.viewer.name
list from github.my_pull_requests { item { text .title } }
```

### Field<T>

`Field<T>` is an editable value owned by the rendered surface. A `field`, `toggle`, `select`,
`slider`, and the rest bind to a field. The field holds what the person typed or chose. Its type
decides the control and the keyboard. A field's value lives as long as the node that owns it survives
reconciliation, so a streamed update does not discard a half typed entry.

```
toggle "Notify me" settings.notify
slider "Volume" vol min 0 max 100
select "Repo" repo options repos
```

### Action

`Action` is an event the runtime accepts, written with `->`. An action gathers the typed fields in
its scope into its input. The host derives argument types from those references and checks them
against its published Action contract. It validates an incoming event before handing it on.
The model does not declare a separate contract.

```
button "Create" -> CreateReminder{title, when, repeat}
```

Some actions are built in. The runtime handles them and the model never sees their events: `Open`,
`Back`, `Dismiss`, `Copy`, `Retry`, `Cancel`. Every other action must be published by the host.
Unknown actions fail closed. The exact argument contracts are in [grammar.md](grammar.md).

## Selection yields a value

A `list`, `table`, or `grid` with `select one` or `select many` exposes a `selected` value in scope.
An action reads it the same way it reads a field.

```
table from github.my_pull_requests select one { column "Title" .title }
button "Open" -> Open{url: selected.url}
```

`selected` is optional row data for `select one` and a list of rows for `select many`.
It is published in the collection's immediate parent block.
That block can contain only one selectable collection.
Template selections do not escape their template instance.
An action needing a single selected row is disabled while no row is selected.
An action that reads a field off `selected` is checked against the row type at compile time.

## Who owns which state

State splits three ways and never overlaps.

| State | Examples | Owner |
|---|---|---|
| Interaction | focus, scroll offset, pressed button, an in progress field edit, which tab is open, an expanded card | the runtime |
| Data and contracts | the value behind every `Value<T>`, binding subscriptions, the current surface, its history, event validation | the host that drives the surface |
| Meaning | what to show next, which surface answers an event | the model |

The runtime owns transient interaction state so a re render never loses focus or scroll. The host
owns data so the model never fetches. The model owns meaning. When an event arrives, the model does
not patch the screen. It emits the next whole surface, and the host diffs it against the current one
so matching nodes keep their interaction state.

There is no declared store in matrix source. There are no routes. There are no expressions, scripts,
timers, or network calls in matrix. A surface is a value, and the model is the thing that produces
the next value.

In mote, the host is the daemon and the runtime is a separate consumer.
The host publishes row types, bindings and action contracts before source validation.
Source does not declare record types.

## Events

1. A person acts on an interactive node.
2. The runtime forms the v0 event with `event` and `args`.
   Node identity and surface revision are host validation context.
3. The host validates the event against the action's derived contract and the surface revision. A
   mismatch is rejected with a typed diagnostic and never reaches the model.
4. A valid event goes to the model as the input to produce the next surface.
5. The button that sent the action shows a pending state until the host acknowledges. A repeated tap
   is ignored while pending.

Pending and failure are exposed through the action binding. The model does not author a pending node
or a failure node per surface. The runtime renders those states from the action's status.

## Streaming and failure

A surface arrives as a stream. The rules keep a partial surface safe to show.

- The host buffers each candidate until its whole source, bindings and lowering validate.
- A validated bound text value may extend while it streams.
- An incomplete candidate is never an interactive surface.
- The system fails closed. An unknown element, property, binding, action, or type is rejected.
- If the final stream is invalid, the last valid committed surface stays on screen.
- Repair is syntactic and bounded. The host never guesses missing meaning.
- Every rejection is a stable machine readable diagnostic with a code, a source span, and the tree
  path where one exists. The diagnostic is what goes back to the model on a retry.

## Identity and reconciliation

A node is identified by its path and type. When a collection can reorder, the model adds a `key` from
the row type so the runtime can match rows across updates and animate a move rather than a replace.
Key is the only identity concept a model writes, and only when path and type are not enough.

```
list from github.my_pull_requests key number { item { text .title } }
```

Matching nodes keep focus, scroll, and in progress edits across a re render. This is why a streamed
update does not disturb what the person is doing.
Key values must be stable strings or integers and unique within one collection snapshot.
Duplicate keys reject the update.
Only keys and state transitions drive animation.
