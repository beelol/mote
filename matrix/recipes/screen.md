# screen

The full surface and its parts. Exactly one screen is the root. Header and footer anchor.

## Make it

```
screen "Inbox" {
  header { row between { title "Inbox" button "Compose" primary -> Compose } }
  section "Unread" {
    list from mail.unread {
      item { text .subject caption .from }
    }
  }
  footer { button "Archive all" -> ArchiveAll }
}
```

Group with a card that can collapse:

```
card "Details" collapsible {
  kv "Repo" .repo
  kv "Author" .author.login
}
```

## Slice

- `screen`, `section`, and `card` take an optional title literal. `card` takes an optional
  `collapsible`.
- `header` and `footer` take no data. They anchor to the top and bottom of the screen.

## Free

- The body between the header and footer scrolls. The header and footer stay in place in the safe
  area.
- The standard padding for each container, from the theme.
- The banner, section, and group accessible roles.

## Animates

- A collapsible card expands and collapses its body.
- A new screen crossfades in while matching keyed nodes carry over from the previous screen.
