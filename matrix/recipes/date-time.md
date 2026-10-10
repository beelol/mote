# date and time

Pickers for a date and a time of day.

## Make it

```
date "When" when
time "Start" start
```

Bound with constraints:

```
date "Due" due min today
```

## Slice

- `when` is a `Field<date>`. `start` is a `Field<time>`.
- `min` and `max` are optional bounds for a date.
- A label literal names the picker.

## Free

- A standard calendar for a date and a standard clock for a time.
- Locale formatting of the shown value.
- The accessible role and the label as its name.
- Keyboard entry as well as the picker.

## Animates

- The calendar or clock expands from the control and collapses back.
- The month or selection crossfades as it changes.
