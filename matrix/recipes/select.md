# select

A pull down menu. Pick one value from a closed set.

## Make it

```
select "Talk model" talk options ["qwen3:4b", "qwen3:0.6b"]
```

The options may be a literal set, as above, or a bound list:

```
select "Repo" repo options github.my_repos
```

## Slice

- `talk` is a `Field<T>` over a closed value type.
- `options` is that same closed set, as a literal set or a `Value<List<T>>`.
- A label literal names the control.

The options and the field share one value type. The runtime rejects an option that is not in the
type before the surface renders.

## Free

- The pull down open and close behavior, the standard height, and the touch target.
- The accessible combobox role and the label as its name.
- Keyboard: arrows move, enter chooses, escape closes.

## Animates

- The menu expands from the control and collapses back into it.
- The chosen row settles as the menu closes.

## Variants

Same closed value idea, different size.

Radio group, when the choices should all be visible:

```
choice "Repeat" repeat options ["none", "daily", "weekly"]
```

Segmented, for a few inline options:

```
segmented "Theme" theme options ["system", "light", "dark"]
```
