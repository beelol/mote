# list

A vertical list of items from one typed list binding. Write one item template; the runtime repeats
it per row.

## Make it

```
list from mail.unread {
  item {
    text .subject
    caption .from
  }
  empty "Nothing unread"
}
```

With stable identity for reordering:

```
list from board.cards key id {
  item { text .title }
}
```

## Slice

- `from` is a `Value<List<T>>`. The row type T is in scope inside `item`.
- `.field` projects a field of the row.
- `key` is an optional stable identity from the row type, for reordering.
- `empty "..."` is optional and only changes the empty message.

## Free

- Scrolling inside the list box, with scroll position kept across a re render.
- Skeleton rows while the binding loads.
- The standard empty message with zero rows, unless `empty` overrides the words.
- The standard error with a retry if the binding fails.
- The list and listitem accessible roles.

## Animates

- A new row fades and slides in. A removed row fades out.
- With `key`, a row that moves tweens from its old slot to its new slot instead of being replaced.
