# media

Images, avatars, icons, times, and rules. Small display elements that are not text.

## Make it

An image the runtime fetches and caches:

```
image media.current.cover alt "Album cover"
```

A small round image for a person or repo:

```
avatar media.current.author.avatar alt media.current.author.login
```

A theme glyph for a fixed semantic name:

```
icon "check"
```

A time, formatted, optionally relative:

```
timestamp media.current.updated relative
```

A thin rule between blocks:

```
divider
```

## Slice

- `media.current` is a host published `Value<Media>` with `cover: asset`,
  `author: { avatar: asset, login: str }` and `updated: datetime`.
- These fragments use root Value paths and require no enclosing collection.
- `image` and `avatar` take a `Value<asset>` as the source and a required `alt`. The runtime fetches
  the asset. The model never fetches.
- `icon` takes a fixed name from [../grammar.md](../grammar.md).
- The theme draws that name and an unknown name fails before render.
- `timestamp` takes a `Value<datetime>` and an optional `relative`.
- `divider` takes nothing.

## Free

- Caching, placeholder while loading, and a broken state if the fetch fails.
- The image accessible role with the alt as its name.
- Locale formatting of the time, and the relative phrasing such as "2 hours ago."

## Animates

- An image crossfades in from its placeholder when it loads.
- A relative timestamp updates in place as time passes.
