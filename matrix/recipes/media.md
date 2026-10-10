# media

Images, avatars, icons, times, and rules. Small display elements that are not text.

## Make it

An image the runtime fetches and caches:

```
image .cover alt "Album cover"
```

A small round image for a person or repo:

```
avatar .author.avatar alt .author.login
```

A glyph from the theme icon set:

```
icon "check"
```

A time, formatted, optionally relative:

```
timestamp .updated relative
```

A thin rule between blocks:

```
divider
```

## Slice

- `image` and `avatar` take a `Value<asset>` as the source and a required `alt`. The runtime fetches
  the asset. The model never fetches.
- `icon` takes a name from the theme icon set. An unknown name fails before render.
- `timestamp` takes a `Value<datetime>` and an optional `relative`.
- `divider` takes nothing.

## Free

- Caching, placeholder while loading, and a broken state if the fetch fails.
- The image accessible role with the alt as its name.
- Locale formatting of the time, and the relative phrasing such as "2 hours ago."

## Animates

- An image crossfades in from its placeholder when it loads.
- A relative timestamp updates in place as time passes.
