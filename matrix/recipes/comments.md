# comments thread

What a person needs to follow a discussion. A list of comments, each with its author and time, and
a box to add one. Replies are a nested list, not an indent the model controls.

## Make it

```
screen "Comments" {
  list from thread.comments key id {
    item {
      row { avatar .author.avatar alt .author.login text .author.login caption .at relative }
      md .body
      list from .replies key id {
        item {
          row { avatar .author.avatar alt .author.login text .author.login }
          md .body
        }
      }
    }
    empty "No comments yet"
  }
  footer { compose draft send -> AddComment{text: draft} }
}
```

## Slice

- `thread.comments` is a `Value<List<Comment>>`. A comment has `author: { login, avatar }`,
  `at: datetime`, `body: str`, and `replies: List<Comment>`.
- `draft` is a `Field<str>`; `AddComment{text}` posts it.

## Free

- Markdown body rendering in the constrained subset, relative time, scroll, empty state, and a
  pending state on send.

## Animates

- A new comment slides up from the compose bar. A reply fades into its nested list.
