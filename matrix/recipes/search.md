# search results

What a person needs when they ask "find X." A search field and a ranked list of results, each with
enough to recognize it and open it.

## Make it

```
screen "Search" {
  header { search "Search" query send -> Search{query} }
  list from search.results key id {
    item {
      text .title
      caption .snippet
      row { tag .source timestamp .updated relative }
      -> Open(.url)
    }
    empty "No matches"
  }
}
```

## Slice

- `query` is a `Field<str>` bound to the search box. `Search{query}` runs the search.
- `search.results` is a `Value<List<Result>>` with `title: str`, `snippet: str`, `source: str`,
  `updated: datetime`, `url: url`.

## Free

- The search box filters as the host returns results, with a skeleton while a query runs, the empty
  state for no matches, and scroll.

## Animates

- Results crossfade as the query changes; `key id` tweens results that stay.
