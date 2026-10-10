# kanban board

What a person needs when they ask "show the board." Columns by state, each a list of cards. A move
is an action, not a drag the model authors; the runtime owns the drag gesture.

## Make it

```
screen "Board" {
  row nowrap {
    col from board.columns {
      row between { text .name badge .count }
      list from .cards key id select one {
        item {
          text .title
          row { avatar .assignee.avatar alt .assignee.name badge .label }
        }
        empty "Empty"
      }
    }
  }
  footer { button "Move" -> MoveCard(id: selected, to: next) }
}
```

## Slice

- `board.columns` is a `Value<List<Column>>` with `name: str`, `count: str`, and
  `cards: List<Card>`. A card has `id`, `title: str`, `label: str`, `assignee: { name, avatar }`.
- `MoveCard` takes the selected card id and a target column.

## Free

- Each column scrolls, the row of columns scrolls sideways with `nowrap`, selection and empty states
  come from the runtime. The drag gesture to move a card is a runtime affordance; the model only
  defines the `MoveCard` action.

## Animates

- A moved card tweens from its old column to its new one through `key id`.
