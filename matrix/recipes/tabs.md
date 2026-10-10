# tabs

A set of labeled panels, one visible at a time.

## Make it

```
tabs {
  tab "Open" {
    list from github.open_pull_requests { item { text .title } }
  }
  tab "Merged" {
    list from github.merged_pull_requests { item { text .title } }
  }
}
```

## Slice

- `tabs` takes no data. It holds `tab` children only.
- Each `tab` takes a title literal and any children.

## Free

- Which tab is open is interaction state the runtime owns. It survives a re render.
- The accessible tablist, tab, and tabpanel roles.
- Keyboard: arrows move between tabs, enter opens one.

## Animates

- The active underline slides to the chosen tab.
- The panels crossfade or slide as the selection changes.
