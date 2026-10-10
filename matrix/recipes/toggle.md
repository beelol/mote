# toggle

An on or off switch.

## Make it

```
toggle "Notify me" notify
```

## Slice

- `notify` is a `Field<bool>`. It holds the on or off state.
- A label literal names the switch.

## Free

- Standard control height and the 48 pixel touch target.
- The accessible switch role and the label as its name.
- Keyboard: space flips it when focused.

## Animates

- The knob slides between off and on.
- The track color crosses between the two theme states.
