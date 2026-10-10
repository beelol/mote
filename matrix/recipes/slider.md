# slider

A value dragged along a range.

## Make it

```
slider "Volume" volume min 0 max 100 step 5
```

## Slice

- `volume` is a `Field<number>`. It holds the current value.
- `min` and `max` bound the range. `step` is optional and defaults to a continuous drag.
- A label literal names the control.

## Free

- The track, the thumb, the standard height, and the touch target.
- The current value shown beside the label.
- The accessible slider role with min, max, and value, and the label as its name.
- Keyboard: arrows nudge by step, home and end jump to the ends.

## Animates

- The thumb follows the drag and settles to the nearest step on release.
- The filled part of the track grows and shrinks with the value.

## Variant

A stepper, when the value is nudged rather than dragged:

```
stepper "Quantity" qty min 1 max 99
```
