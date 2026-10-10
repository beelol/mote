# pull requests

The worked inferred UI case. A person asks "what are my pull requests," and an agent answers with a
real, actionable surface. The agent decides what to show, not how to lay it out.

## Start from the question

To recognize and act on a pull request in a list, a person needs a small slice:

- The title, so they know what it is.
- The repository and number, so they know where it lives.
- The state: open, draft, merged, or closed. A status.
- The review state: approved, waiting, or changes requested. A status.
- The check state: passing, failing, or running. A status.
- When it last moved, as a relative time.
- The author, as a small avatar.
- One action: open it.

Nothing else is needed to answer the question. Diffs, files, and commits belong to a detail surface
the person reaches by opening one.

## Make it

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
          row { badge .review badge .checks }
        }
      }
      caption .updated relative
    }
    empty "No open pull requests"
  }
  footer { button "Open" primary -> Open(url.of(selected)) }
}
```

## What the runtime supplied

The agent wrote the slice and the shape. Free: the scroll, the loading skeletons, the empty message,
the touch targets, the badge colors from the status tones, the relative time, and the selection
behavior. The `state`, `review`, and `checks` values mapped to tones on their own, so open is
neutral, a passing check is success, a failing check is danger, and a draft or a running check is
pending. No color was written. On a reorder, `key number` lets each row tween to its new slot.

## The data the host supplies

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
