# media gallery

What a person needs when they ask "show the photos." A wrapping grid of thumbnails they can open.
This is the `grid` collection applied to images.

## Make it

```
screen "Photos" {
  grid from photos.recent key id select one {
    tile {
      image .thumb alt .caption
      caption .caption
    }
  }
  footer { button "Open" primary -> Open{url: selected.url} }
}
```

## Slice

- `photos.recent` is a `Value<List<Photo>>` with `thumb: asset`, `caption: str`, `url: url`,
  `id`.

## Free

- The column count follows the surface width, thumbnails load with placeholders, selection and the
  empty state come from the runtime.

## Animates

- A new tile fades and scales in. Tiles reflow when the width changes; `key id` tweens the ones that
  stay.
