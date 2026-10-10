# file browser

What a person needs when they ask "show me this folder." A breadcrumb of where they are and a list
of entries they can open.

## Make it

```
screen "Files" {
  header {
    list from files.breadcrumbs {
      item { link .label to .url }
    }
  }
  list from files.here key path select one {
    item {
      row {
        text .kind
        text .name fill
        caption .size
        timestamp .modified relative
      }
    }
    empty "Empty folder"
  }
  footer { button "Open" primary -> Open{url: selected.url} }
}
```

## Slice

- `files.here` is a `Value<List<Entry>>`. An entry has `name: str`, `kind: folder | file | image`,
  `size: str`, `modified: datetime`, `path: str`, `url: url`.
- The header reads `files.breadcrumbs`, a `Value<List<Crumb>>` of `label: str` and `url: url`.
- The host validates resource URLs for `Open` rather than passing raw paths.

## Free

- Scroll, selection, relative time, empty state.

## Animates

- Entering a folder crossfades the list; `key path` tweens entries that persist.
