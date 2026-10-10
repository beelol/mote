# multiselect

A pull down that accepts many values from a closed set.

## Make it

```
multiselect "Labels" labels options github.repo_labels
```

## Slice

- `labels` is a `Field<List<T>>` over a closed value type.
- `options` is that same closed set, as a literal set or a `Value<List<T>>`.
- A label literal names the control.

## Free

- The pull down, the standard height, and the touch target.
- Chosen values show as removable chips inside the control.
- The accessible listbox role with multiselect, and the label as its name.
- Keyboard: arrows move, space toggles a value, escape closes.

## Animates

- The menu expands and collapses like a select.
- A chosen chip enters by fade and a removed chip leaves by fade.
