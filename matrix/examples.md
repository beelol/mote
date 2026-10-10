# matrix examples

Every example is matrix source. Read these to see how short a real surface is. The catalog and the
bindings documents define every word used here.
[grammar.md](grammar.md) defines the exact forms.
Cases without `screen` are fragments inserted into an untitled screen for rendering.
Host record descriptions and event JSON below are not Matrix source.

## Conformance suite

These ten cases are the minimum a matrix implementation must render identically. They exist to pin
the language before any parser is written.

### 1. Static text in a column

```
screen "About" {
  title "mote"
  text "An always on personal agent you own."
  caption "Working name."
}
```

### 2. A row that wraps

```
row {
  tag "rust"
  tag "local models"
  tag "offline"
  tag "accessible"
}
```

On a narrow surface the tags wrap to the next line. The model wrote no breakpoint.

### 3. A typed field, toggle, select, and slider

```
col {
  field "Display name" display_name kind text min 2
  toggle "Notify me" notify
  select "Talk model" talk options ["qwen3:4b", "qwen3:0.6b"]
  slider "Volume" volume min 0 max 100 step 5
}
```

Every control shares the standard height from the theme, so the column lines up with no effort.

### 4. An action that gathers typed fields

```
form "New reminder" {
  field "Title" title kind text min 3
  date "When" when
  select "Repeat" repeat options ["none", "daily", "weekly"]
  button "Create" primary -> CreateReminder{title, when, repeat}
}
```

The event the runtime sends on submit:

```
{ "event": "CreateReminder",
  "args": { "title": "Call Ana", "when": "2026-10-10", "repeat": "none" } }
```

### 5. A collection bound to a typed list

```
list from mail.unread {
  item {
    text .subject
    caption .from
  }
  empty "Nothing unread"
}
```

### 6. Loading, empty, pending, and failure

```
list from mail.unread {
  item { text .subject }
}
```

The same source covers four runtime states with no extra words. While `mail.unread` loads, the list
shows skeleton rows. With zero rows it shows the standard empty message. A button that sends an
action shows a pending state until the host acknowledges. If the binding fails, the list shows the
standard error with a retry. A model only writes `empty "..."` or `error "..."` to change the words.

### 7. An inline chat card and the same content full screen

Inline, inside another surface:

```
card "Assistant" {
  chat from thread.messages {
    message role .role body .body
  }
  compose draft send -> Send{text: draft}
}
```

Full screen is the same block as the root of a `screen`. The content does not change. The runtime
gives the full screen version its own scroll and anchors the compose bar as a footer.

### 8. A stable key reorder

```
list from board.cards key id {
  item { text .title }
}
```

When the host sends a new surface with the cards in a new order, the `key id` lets the runtime match
each card and animate the move instead of replacing the rows.

### 9. An anchored header and footer

```
screen "Inbox" {
  header { row between { title "Inbox" button "Compose" primary -> Compose } }
  list from mail.unread {
    item { text .subject caption .from }
  }
  footer { button "Archive all" -> ArchiveAll }
}
```

The header and footer stay in place within the safe area while the list scrolls between them.

### 10. A malformed binding fails before render

```
button "Open" -> Open{url: selected.url}
```

Written without a `select one` collection in scope, `selected` is not defined. The binding fails type
checking and the surface never renders. The diagnostic returned to the model:

```
{ "code": "E011", "message": "symbol not in scope: selected",
  "span": { "line": 1, "col": 28 }, "path": "button[0]" }
```

## Real world: what are my pull requests

This is the target use. A person asks "what are my pull requests," and an agent answers with a real,
actionable surface. The value of matrix is that the agent only decides what to show, not how to lay
it out.

### What you actually need to view a pull request minimally

Start from the question, not the GitHub API. To recognize and act on a pull request in a list, a
person needs a small slice:

- The title, so they know what it is.
- The repository and number, so they know where it lives.
- The state: open, draft, merged, or closed. This is a status, shown as a badge tone.
- The review state: whether it is approved, waiting, or has changes requested.
- The check state: whether CI is passing, failing, or running.
- When it last moved, as a relative time.
- The author, as a small avatar.
- One action: open it.

That slice is everything. Nothing about the surface needs branch diffs, file lists, or commit
history to answer "what are my pull requests." Those belong to a detail surface the person reaches by
opening one.

### The surface in matrix

```
screen "Pull requests" {
  list from github.my_pull_requests key number select one {
    item {
      row between {
        row {
          avatar .author.avatar alt .author.login
          col {
            text .title
            caption .repo code .number
          }
        }
        col end {
          badge .state
          row {
            badge .review
            badge .checks
          }
        }
      }
      timestamp .updated relative
    }
    empty "No open pull requests"
  }
  footer { button "Open" primary -> Open{url: selected.url} }
}
```

The agent wrote the slice and the shape. The runtime supplied the scroll, the loading skeletons, the
empty message, the touch targets, the badge colors from the status tones, the relative time
formatting, and the selection behavior. The `state`, `review`, and `checks` values map to status
tones automatically, so open is neutral, a passing check is success, a failing check is danger, and a
draft or a running check is pending. No color was written.

### The data the host must supply

matrix names `github.my_pull_requests` as a `Value<List<PullRequest>>`. The host owns the fetch. The
row type the surface relies on is small.
The host publishes this record in its manifest.
This notation documents that record and is not a source declaration.

```
PullRequest {
  number: int
  title: str
  repo: str
  state: open | draft | merged | closed
  review: approved | changes_requested | review_required | none
  checks: success | failure | pending | none
  updated: datetime
  author: { login: str, avatar: asset }
  url: url
}
```

This is the contract between matrix and the host for this surface. The agent did not invent it. It
used the fields its surface referenced, and the host provides exactly those.

## Real world: a settings screen

```
screen "Settings" {
  section "Models" {
    select "Talk model" talk options model_list
    select "Think model" think options model_list
    slider "Context window" num_ctx min 2048 max 32768 step 1024
  }
  section "Behavior" {
    toggle "Local only" local_only
    toggle "Announce replies" announce
    segmented "Theme" theme options ["system", "light", "dark"]
  }
  footer { button "Save" primary -> SaveSettings{talk, think, num_ctx, local_only, announce, theme} }
}
```

## Real world: a confirmation dialog

```
dialog "Delete 3 invoices" {
  text "This cannot be undone."
  row end {
    button "Cancel" -> Dismiss
    button "Delete" danger confirm -> DeleteInvoices{rows: invoices.pending_delete}
  }
}
```

`Dismiss` is a built in action the runtime handles. `DeleteInvoices` is a model action with a derived
contract accepting the host's list of invoices pending deletion.

## Measurement and rejection coverage

[measurement.md](measurement.md) records source bytes and approved local tokenizer counts.
It distinguishes exact v0 equivalents from unsupported design cases.
There is no Matrix parser yet, so the snippets are grammar fixtures rather than execution evidence.
An implementation must reject unknown words, modifiers, bindings and key fields.
It must also reject malformed strings, unbalanced braces, invalid child placement, duplicate keys,
multiple selectable collections in one scope, helper calls and unknown Action arguments.
Rejection must preserve the last valid surface.
