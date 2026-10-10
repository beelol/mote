# dashboard

What a person needs when they ask "how is it going." The minimal answer is a row of headline
numbers and one or two tables, not a wall of charts.

## Start from the question

To read status at a glance a person needs a few key numbers with their trend, and the short list of
things that need attention. Nothing else earns a place above the fold.

## Make it

```
screen "Overview" {
  row {
    stat "Open PRs" github.pull_request_count
    stat "Failing checks" fails delta fails_trend
    stat "Latency p50" latency delta latency_trend
  }
  section "Needs attention" {
    table from alerts.open {
      column "Alert" .title
      column "Since" .since kind date
      column "State" .state kind status
    }
  }
}
```

## Slice

- Each `stat` takes a label and a `Value<str>`, and an optional `delta` for the trend.
- The table reads a `Value<List<Alert>>` with `title`, `since`, and `state`.

## Free

- The stat numbers roll to new values, trends take success or danger tones, the table sorts and
  shows empty and error states, all from the runtime.

## Gap note

Charts are deferred. A `stat` and a `table` answer the status question. Reconsider only when a
required dashboard cannot be read without a trend line. See decisions.md.
