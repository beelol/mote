# matrix themes

A theme is the one token set a surface inherits. It is the only place color, spacing, padding,
heights, type, radius, elevation, and motion are decided. A model never writes any of these, and a
model never picks a theme. The runtime selects the active theme. See
[../layout-and-theme.md](../layout-and-theme.md) for the rule and the hook, and
[../runtime.md](../runtime.md) for how the runtime applies the motion.

## The contract every theme meets

A theme is a base theme, not a style for one screen. Each theme renders every element in
[../catalog.md](../catalog.md) and defines:

- The spacing scale, the padding scale, and the one control height.
- The type scale for each text element: `title`, `text`, `caption`, `code`.
- The corner radius and the one elevation for cards and overlays.
- The color roles for light and dark: surface, text, muted text, line, and the one accent.
- The six status tones: `neutral`, `info`, `success`, `warning`, `danger`, `pending`.
- Glyphs for the fixed semantic icon names in [../grammar.md](../grammar.md).
- The one motion duration and curve the runtime uses for every state transition.

Because every element reads these from the theme, swapping the theme re spaces, re colors, and re
animates the whole interface at once, and no matrix source changes.

## Themes

- [Vector](vector.md). The default theme. Thin retro red, Atari vector line. This is what a surface
  inherits when the runtime defines no other theme.
