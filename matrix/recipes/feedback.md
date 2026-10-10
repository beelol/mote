# feedback

Progress, stages, typing, and the empty and error states. Most of these the runtime shows on its
own. The model writes one only to change the words.

## Make it

A determinate bar or an indeterminate spinner:

```
progress value job.percent label "Indexing"
```

A sequence of stages with a current one:

```
steps stages ["Draft", "Review", "Merged"] current workflow.stage
```

A chat typing indicator:

```
typing
```

Override an empty or error message:

```
list from mail.unread {
  item { text .subject }
  empty "Nothing unread"
  error "Mail is unreachable" -> Retry
}
```

## Slice

- `progress` takes an optional `Value<number>` and an optional label. No value means a spinner.
- `steps` takes a literal set of stages and a `Value<str>` current stage.
- `workflow.stage` is the host published stage name.
- `typing` takes nothing.
- `empty` and `error` take a literal message. `error` takes an optional `-> Retry`.

## Free

- A bound value shows skeletons while loading and a standard error with a retry if it fails. The
  model does not write these to get them.
- The progressbar, status, and alert accessible roles.

## Animates

- A determinate bar grows to its value. A spinner rotates continuously.
- The current step advances along the sequence.
- A skeleton crossfades to real content when the data arrives.
