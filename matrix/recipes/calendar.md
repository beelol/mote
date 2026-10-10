# calendar

What a person needs when they ask "what is on my calendar today" or "this week." The minimal answer
is an agenda, not a pixel time grid. matrix has no absolute positioning, so events are a list in time
order, not blocks placed by the clock. This is the inferred minimal format on purpose.

## Day

```
screen "Today" {
  list from calendar.today {
    item {
      row {
        timestamp .start
        col {
          text .title
          caption .location
        }
        badge .status fill end
      }
    }
    empty "Nothing scheduled"
  }
}
```

## Week

Seven day columns, each an agenda. A `row` of `col`, one per day.

```
screen "This week" {
  row {
    col from calendar.week {
      text .weekday
      list from .events {
        item { timestamp .start text .title }
        empty "Free"
      }
    }
  }
}
```

## Slice

- `calendar.today` is a `Value<List<Event>>`. An event has `start: datetime`, `title: str`,
  `location: str`, `status: confirmed | tentative | canceled`.
- `calendar.week` is a `Value<List<Day>>`, each with `weekday: str` and `events: List<Event>`.

## Free

- Time order, relative or clock formatting, scroll, empty state, status tones.

## Animates

- A new event fades into its time slot. A canceled event fades its row to the canceled tone.

## Gap recorded

A true time grid, where an event's height is its duration and its top is its start, needs
proportional placement, which matrix forbids. The agenda list is the accepted representation. See
`decisions.md` under deferred.
