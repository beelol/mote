# field

A single line text entry. The kind sets the keyboard and the validation.

## Make it

```
field "Display name" display_name kind text min 2
```

Kinds set behavior:

```
field "Email" email kind email
field "Port" port kind number min 1 max 65535
field "Website" site kind url
field "Password" secret kind password
```

## Slice

- The bound name is a `Field<str>` for text kinds and a `Field<number>` for the number kind.
- `kind` is one of text, email, url, number, phone, password.
- `min`, `max`, and `pattern` are optional constraints.
- A label literal names the field.

## Free

- The standard height, the touch target, and the right soft keyboard per kind.
- Validation on blur and on submit. A failing field shows its message and blocks its form's submit.
- The accessible textbox role and the label as its name.

## Animates

- The label floats as the field gains content.
- A validation message slides in under the field and out when fixed.

## Variants

Multi line entry:

```
area "Notes" notes
```

A field that filters a nearby collection:

```
search "Filter" query
```
