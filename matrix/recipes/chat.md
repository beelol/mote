# chat

A scrolling transcript of messages and a compose bar.

## Make it

```
chat from thread.messages {
  message role .role body .body
}
compose draft send -> Send{text: draft}
```

Inline, inside another surface, wrap it in a card:

```
card "Assistant" {
  chat from thread.messages { message role .role body .body }
  compose draft send -> Send{text: draft}
}
```

## Slice

- `chat` takes `from`, a `Value<List<Message>>`. The message type is in scope inside `message`.
- `message` takes a `role` of me, them, system or a bound value of that closed enum.
- Its `body` is a typed string projection.
- `compose` takes a `Field<str>` and a `send` action that gathers the text.

## Free

- Scrolling pinned to the newest message, with position kept when older messages load.
- Alignment and tone per role, from the theme.
- Timestamps on hover or tap. Copy on long press.
- The log and listitem accessible roles.

## Animates

- A new message slides up from the compose bar.
- Streamed message text extends in place as it arrives.
- The typing indicator pulses while a reply streams.
