# notifications

What a person needs when they ask "what did I miss." A list of recent items, unread ones marked,
each openable, with a way to clear them.

## Make it

```
screen "Notifications" {
  header { row between { title "Notifications" button "Mark all read" -> MarkAllRead } }
  list from notifications.recent key id {
    item {
      row {
        avatar .source.icon alt .source.name
        col fill {
          text .title
          caption .body
        }
        col end {
          timestamp .at relative
          badge .state
        }
      }
      button "Open" -> Open{url: .url}
    }
    empty "You are all caught up"
  }
}
```

## Slice

- `notifications.recent` is a `Value<List<Notice>>`. A notice has `title: str`, `body: str`,
  `id: str`, `at: datetime`, `state: unread | read`, `source: { name, icon }`, `url: url`.

## Free

- Unread tone from `state`, relative time, scroll, empty state. Opening one is the built in `Open`.

## Animates

- A read item fades its unread tone. A removed item fades out; `key id` tweens the rest up.
