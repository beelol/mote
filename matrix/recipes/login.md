# login

What a person needs to sign in. A small form with the right field kinds and one submit. Failure
comes back through the action, not a node the model writes.

## Make it

```
screen "Sign in" {
  col center {
    title "mote"
    form {
      field "Email" email kind email
      field "Password" password kind password
      button "Sign in" primary -> SignIn{email, password}
    }
    link "Forgot password" to reset.url
  }
}
```

## Slice

- `email` is a `Field<str>` of kind email. `password` is a `Field<str>` of kind password.
- `SignIn{email, password}` gathers both. Its derived contract is `{ email: str, password: str }`.

## Free

- The email and password keyboards, validation on blur and submit, the submit disabled until valid,
  and a pending state on the button after a tap.
- A rejected `SignIn` shows the host's error inline through the action. The model does not author a
  failure node.

## Animates

- The button shows a pending spinner, then returns. An invalid field shakes once as its message
  slides in.
