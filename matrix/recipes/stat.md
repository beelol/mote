# stat and key value

A big number with a label, and a label with its value on one line.

## Make it

A stat, for a dashboard number:

```
stat "Open PRs" github.pull_request_count
```

With a change indicator:

```
stat "Latency p50" latency delta trend
```

A key value, for a detail row:

```
kv "Repo" github.pull_request.repo
kv "Author" github.pull_request.author.login
```

## Slice

- `github.pull_request` is a host published `Value<PullRequest>` with `repo: str`
  and `author: { login: str }` for these detail fragments.
- These fragments use root Value paths and require no enclosing collection.
- `stat` takes a label literal and a typed string or number value.
- Its optional delta accepts the same type choices.
- `kv` takes a label literal and a typed string, number or closed enum value.

## Free

- The large type for the stat number and the quiet type for its label, from the theme scale.
- A delta shows a success or danger tone by its sign, through the status tones.
- The group accessible role.

## Animates

- A stat number rolls from its old value to its new value when it changes.
- The delta arrow and color cross when the trend flips.
