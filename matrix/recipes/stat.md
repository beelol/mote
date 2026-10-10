# stat and key value

A big number with a label, and a label with its value on one line.

## Make it

A stat, for a dashboard number:

```
stat "Open PRs" count.of(github.my_pull_requests)
```

With a change indicator:

```
stat "Latency p50" latency delta trend
```

A key value, for a detail row:

```
kv "Repo" .repo
kv "Author" .author.login
```

## Slice

- `stat` takes a label literal and a `Value<str>`, and an optional `delta` as a `Value<str>`.
- `kv` takes a label literal and a `Value<str>`.

## Free

- The large type for the stat number and the quiet type for its label, from the theme scale.
- A delta shows a success or danger tone by its sign, through the status tones.
- The group accessible role.

## Animates

- A stat number rolls from its old value to its new value when it changes.
- The delta arrow and color cross when the trend flips.
