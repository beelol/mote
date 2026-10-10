# text

Plain text, always. Never markup. The element picks the size from the theme type scale.

## Make it

```
title "Pull requests"
text "An always on personal agent you own."
caption "Updated a moment ago"
code "qwen3:4b"
```

Bound text reads a value:

```
text github.viewer.name
```

A constrained Markdown subset, for model prose:

```
md message.body
```

A link the runtime opens:

```
link "Open on GitHub" to github.pull_request.url
```

A short keyword chip:

```
tag "rust"
```

## Slice

- `github.pull_request` is a host published `Value<PullRequest>` with `url: url` for the link.
- These fragments use root Value paths and require no enclosing collection.
- A literal string, or a `Value<str>` for live text.
- `md` takes a `Value<str>` and renders a subset: emphasis, lists, inline code, links. No raw HTML,
  no images, no deep headings.
- `link` takes a label and a `Value<url>`.

## Free

- Wrapping, truncation with a tooltip, and copy on long press.
- The heading, text, or code accessible role per element.
- The size and weight from the theme, never from the model.

## Animates

- Streamed text extends in place as it arrives, without reflowing the surface around it.
