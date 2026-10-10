# selection

Let a person pick rows from a collection, then act on the picked rows.

## Make it

One row, with an action that reads the choice:

```
list from github.my_pull_requests select one {
  item { text .title caption .repo }
}
button "Open" primary -> Open{url: selected.url}
```

Many rows:

```
table from invoices select many {
  column "Invoice" .number
  column "Amount" .amount kind money
}
button "Delete" danger confirm -> DeleteInvoices{rows: selected}
```

## Slice

- `select one` or `select many` on a `list`, `table`, or `grid` turns on selection.
- It exposes `selected` in the collection's immediate parent block.
- For `select one` it is an optional row and actions needing it disable until a row is chosen.
- For `select many` it is a list of the row type.
- Only one selectable collection is allowed in that parent block.
- An action reads fields off `selected`, checked against the row type before render.

## Free

- Checkbox or radio affordances, shift click ranges, and the select all control for `select many`.
- The accessible selected state per row.

## Animates

- A row's selected mark fades in and out.
- An action bar for the selection slides in when the first row is picked and out when none remain.
