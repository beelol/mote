# grid

A wrapping set of equal cards, for tiles and galleries. Like a list, but it wraps into even cells
instead of stacking.

## Make it

```
grid from photos.recent {
  tile {
    image .thumb alt .caption
    caption .caption
  }
}
```

## Slice

- `from` is a `Value<List<T>>`. The row type T is in scope inside `tile`.
- `.field` projects a field of the row.

## Free

- The number of columns follows the surface width. The model never writes a column count.
- Scrolling, skeleton tiles while loading, the standard empty and error states.
- The list and listitem accessible roles.

## Animates

- A new tile fades and scales in. A removed tile fades out.
- Tiles reflow to new positions when the width changes or the set reorders.
