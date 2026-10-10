# file browser

What a person needs when they ask "show me this folder." A breadcrumb of where they are and a list
of entries they can open.

## Make it

```
screen "Files" {
  header { row { link .crumb to .path } }
  list from files.here key path select one {
    item {
      row {
        icon .kind
        text .name fill
        caption .size
        timestamp .modified relative
      }
    }
    empty "Empty folder"
  }
  footer { button "Open" primary -> Open(path.of(selected)) }
}
```

## Slice

- `files.here` is a `Value<List<Entry>>`. An entry has `name: str`, `kind: folder | file | image`,
  `size: str`, `modified: datetime`, `path: str`.
- The header reads a `Value<List<Crumb>>` of `label` and `path` for the breadcrumb.

## Free

- Scroll, selection, icons per kind, relative time, empty state.

## Animates

- Entering a folder crossfades the list; `key path` tweens entries that persist.
