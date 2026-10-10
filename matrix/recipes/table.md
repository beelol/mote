# table

Rows and typed columns, for data a person compares across columns.

## Make it

```
table from github.my_pull_requests {
  column "Title" .title
  column "Repo" .repo
  column "State" .state kind status
  column "Updated" .updated kind date
}
```

## Slice

- `from` is a `Value<List<T>>`. The row type T is in scope for each column's `.field`.
- Each `column` takes a label literal, a `.field` value, and an optional `kind` of text, number,
  money, date, or status.

## Free

- Sorting by a column when its kind is sortable. The header is the control.
- Scrolling, skeleton rows while loading, the standard empty and error states.
- Collapse to stacked cards on a narrow surface.
- The table, row, and column header accessible roles.
- Column formatting per kind: money, date, and the status tones for a status column.

## Animates

- Rows reflow when a sort changes, each row tweening to its new position.
- The sort indicator rotates between ascending and descending.
