# wizard

What a person needs for a multi step task. A progress of stages and the form for the current step.
The model emits the surface for the step the person is on; it does not script the flow.

## Make it

```
screen "Set up" {
  steps stages ["Account", "Profile", "Done"] current setup.step
  form "Profile" {
    field "Display name" display_name kind text min 2
    area "Bio" bio
    row end {
      button "Back" -> Back
      button "Next" primary -> SaveProfile{display_name, bio}
    }
  }
}
```

## Slice

- `steps` takes the literal stage set and a `Value<str>` current stage.
- `setup.step` is the host published current stage.
- Each step is a `form` whose submit action carries that step's fields. `Back` is built in.

## Free

- The current stage advances along the steps. The submit button disables until the step is valid.
  The model shows only the current step; the host advances to the next surface on submit.

## Animates

- The current step marker advances. The next step's form slides in as the previous slides out.

## Note

There are no model authored conditionals. To branch, the host sends the surface for the next
applicable step. This keeps the flow in the host and the model, not in matrix source.
