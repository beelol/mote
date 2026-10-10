# overlays

Things that float above the current screen. The runtime owns the scrim, focus trapping, and
dismissal. The model declares intent.

## Make it

A modal for a decision:

```
dialog "Delete 3 invoices" {
  text "This cannot be undone."
  row end {
    button "Cancel" -> Dismiss
    button "Delete" danger confirm -> DeleteInvoices(ids: selected)
  }
}
```

A panel that slides from an edge for a side task:

```
sheet "Filters" {
  choice "State" state options ["open", "merged", "closed"]
  button "Apply" primary -> ApplyFilters{state}
}
```

A pull down of actions from a trigger:

```
menu trigger icon "more" {
  button "Rename" -> Rename
  button "Delete" danger -> Delete
}
```

Help on one child:

```
tooltip "Copied to clipboard" { icon "check" }
```

## Slice

- `dialog` and `sheet` take a title literal and any children, usually a `form` or buttons.
- `menu` takes a `trigger`, a literal or an `icon`, and `button` children.
- `tooltip` takes a text literal and exactly one child.

## Free

- The scrim, focus trapping inside the overlay, and dismissal by `Dismiss`, escape, or tapping out.
- The dialog, menu, and tooltip accessible roles.

## Animates

- A dialog scales up behind a fading scrim and reverses on dismiss.
- A sheet slides in from its edge and back out.
- A menu expands from its trigger and collapses into it.
