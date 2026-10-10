# form and button

A group of fields with one submit action. The action gathers the fields' typed values.

## Make it

```
form "New reminder" {
  field "Title" title kind text min 3
  date "When" when
  select "Repeat" repeat options ["none", "daily", "weekly"]
  button "Create" primary -> CreateReminder{title, when, repeat}
}
```

A button can also stand alone:

```
button "Archive all" -> ArchiveAll
```

## Slice

- Each explicitly referenced field contributes its `Field<T>` to the action input.
- `-> Action{fields}` names the action and the fields it gathers. The runtime derives the input
  contract from those fields. Here it is `{ title: str, when: date, repeat: none | daily | weekly }`.
- `primary` marks the main button. `confirm` requires a second tap. `disabled` keeps it inert.
- Exactly one primary button inside the form is its submit.
- Only explicitly named Action arguments are gathered.

## Free

- The submit button disables until every field is valid.
- A pending state after a tap until the host acknowledges. A second tap while pending is ignored.
- The accessible form grouping and button role.
- Built in actions the runtime handles without the model: `Back`, `Dismiss`, `Copy`, `Open`,
  `Retry`, `Cancel`.

## Animates

- The pressed button settles, then shows a pending spinner in place, then returns.
- A field that becomes invalid shakes once as its message slides in.
