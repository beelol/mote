# status

Show a state. The model supplies a value. The runtime chooses the tone. The theme chooses the color.

## Make it

A badge, for a short state word:

```
badge .state
```

A banner, for a prominent message with a tone and an optional action:

```
banner "Offline. Showing cached data." tone warn -> Retry
```

A single inline status line, optionally live for assistive technology:

```
status "Syncing" live
```

## Slice

- `badge` takes a literal or a typed display value. If the value names a state, such as a pull request `open`,
  the runtime maps it to a tone on its own.
- `banner` takes a display operand and a `tone` of info, warn, or error, and an optional `-> Action`.
- `status` takes a display operand and an optional `live`.

## The tones

Every status maps to one of six closed meanings: `neutral`, `info`, `success`, `warning`, `danger`,
`pending`. The theme assigns the color to each. The model never writes a color.

## Free

- The tone mapping, the icon per tone, and the accessible status or alert role.
- A `live` status is announced to assistive technology when it changes.

## Animates

- A badge whose tone changes crossfades between colors.
- A banner slides in at the top and out when dismissed or resolved.
