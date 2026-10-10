# matrix recipes

One concept per file. Each file shows how you would make that thing in matrix, then what the runtime
gives you for free and what it animates. Read a recipe when you want to build one thing. Read
[../catalog.md](../catalog.md) for the whole closed catalog, and [../runtime.md](../runtime.md) for
why these affordances are free and animated.
All source uses [../grammar.md](../grammar.md).
Snippets without a screen are fragments inserted into a screen by the host.
Relative `.field` references require the row scope named in the recipe's slice.
Root data uses a host published path rather than an implicit row.

## The recipe shape

Every recipe uses the same four parts.

- **Make it.** The matrix snippet. This is all a model writes.
- **Slice.** The data and bindings the thing needs. Nothing beyond this.
- **Free.** What the immediate mode runtime infers, so the model never writes it.
- **Animates.** What the runtime tweens when the thing enters, leaves, or changes.

## Index

### Inputs
- [toggle](toggle.md)
- [select](select.md), a pull down and its radio and segmented variants
- [multiselect](multiselect.md)
- [slider](slider.md), and the stepper
- [field](field.md), text entry kinds, multi line, and search
- [date and time](date-time.md)
- [form and button](form.md), actions and confirmation

### Display
- [text](text.md), title, caption, code, md, link, tag
- [status](status.md), badge, banner, and the status tones
- [stat](stat.md) and key value
- [media](media.md), image, avatar, icon, timestamp, divider

### Layout
- [layout](layout.md), row, col, and the named roles
- [screen](screen.md), section, card, header, footer
- [tabs](tabs.md)

### Collections
- [list](list.md)
- [table](table.md)
- [grid](grid.md), tiles and galleries
- [selection](selection.md)

### Feedback
- [feedback](feedback.md), progress, steps, typing, empty, error

### Chat
- [chat](chat.md) and compose

### Overlays
- [overlays](overlays.md), dialog, sheet, menu, tooltip

### Patterns
- [pull requests](pull-requests.md), the worked inferred UI case
- [calendar](calendar.md), the agenda representation of a day and a week
- [dashboard](dashboard.md), headline stats and an attention table
- [file browser](file-browser.md), a breadcrumb and an entry list
- [notifications](notifications.md), a recent-activity inbox
- [search](search.md), a query box and ranked results
- [profile](profile.md), an identity header with stats and details
- [kanban](kanban.md), state columns of cards
- [wizard](wizard.md), a multi step form with progress
- [login](login.md), a sign-in form
- [gallery](gallery.md), a wrapping grid of media
- [comments](comments.md), a thread with nested replies and a compose bar
