# layout

Two containers and a handful of named roles. No pixels, no grid tracks, no absolute position.

## Make it

Stack top to bottom:

```
col {
  text "First"
  text "Second"
}
```

Lay out left to right, wrapping when narrow:

```
row {
  tag "rust"
  tag "local"
  tag "offline"
}
```

Push two groups apart, one filling the space:

```
row between {
  title "Inbox"
  button "Compose" primary -> Compose
}
```

Let one child take the remaining space:

```
row {
  field "Search" query fill
  button "Go" -> Search{query}
}
```

## Slice

- `col` and `row` take no data. They only group children.
- Roles are words, never numbers: `fill`, `nowrap`, `start`, `center`, `end`, `between`, `around`,
  `secondary`.

## Free

- The standard gap between children and the standard padding, from the theme.
- Wrapping on a narrow surface for a `row`, unless `nowrap` makes it scroll sideways.
- Scrolling for any container taller than the surface.

## Animates

- When a child enters or leaves, its neighbors slide to close or open the gap.
- A row that rewraps on resize animates its children to their new lines.
