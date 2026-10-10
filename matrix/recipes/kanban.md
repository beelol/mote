# kanban board

What a person needs when they ask "show the board." Columns by state, each a list of cards. A move
is an explicit action on a selected card and a target column.

## Make it

```
screen "Board" {
  grid from board.columns key id {
    tile {
      row between { text .name badge .count }
      list from .cards key id select one {
        item {
          text .title
          row { avatar .assignee.avatar alt .assignee.name badge .label }
        }
        empty "Empty"
      }
      select "Move to" target options board.column_names
      button "Move" -> MoveCard{id: selected.id, to: target}
    }
  }
}
```

## Slice

- `board.columns` is a `Value<List<Column>>` with `id: str`, `name: str`, `count: str` and
  `cards: List<Card>`. A card has `id: str`, `title: str`, `label: str`, `assignee: { name, avatar }`.
- `board.column_names` supplies the target column enum options.
- `target` is a `Field<ColumnName>` instantiated separately in each keyed tile.
- `MoveCard` takes the selected card id and a target column.

## Free

- Each column scrolls and the grid wraps columns on a narrow surface.
- Selection and empty states come from the runtime.
- Selection stays within its tile and the action disables until a card is selected.
- Drag to move is deferred because the current Action contract declares no drop target.

## Animates

- Reordered cards within a column tween through `key id`.
- A move between collections leaves one column and enters another.
