# Custom views

Use **Settings → Open views file**, then save named queries and choose **Reload views**. The file is optional. Inbox, Later and Books remain available if it is missing or invalid.

```json
{
  "views": [
    { "name": "Short essays", "query": "minutes__lte:10 AND (in:inbox OR in:later) AND category:article" }
  ]
}
```

The app evaluates queries against Reader metadata. It supports a bounded subset of Reader syntax; unsupported fields and operators show an error for that view.

| Filter | Supported values / comparisons |
| --- | --- |
| `in:` | `inbox` (alias `new`), `later`, `shortlist`, `archive` |
| `category:` / `type:` | `article`, `epub`, `email`, `pdf`, `tweet`, `rss`, `video` |
| `tag:` | Exact tag name or key; quote names containing spaces |
| `has:` | `tags` |
| `minutes:` | Equality, `__lt`, `__lte`, `__gt`, `__gte` |

Non-numeric filters support `__not`, for example `category__not:video` and `has__not:tags`. Join filters with explicit `AND` or `OR`; parentheses group them, and `AND` binds before `OR`. Matching is case-insensitive. Unknown reading time does not match a minutes filter; it is not estimated from word count.

```text
tag:"Urban design" AND (in:inbox OR in:later)
```

[views.example.json](../views.example.json) includes Quick Reads (up to and including 10 minutes) and Long Reads (over 10 minutes). The examples exclude tagged items and archived content; Books includes tagged EPUBs in Inbox, Later and Shortlist.

Feed is excluded from the import. Saved Home layouts and queries cannot be imported through Reader's documented API. Copy queries into this file once; edit and reload whenever your preferred rules change.

Limits: 30 custom views, names up to 80 characters, queries up to 2,048 characters and 20 nested groups, file up to 64 KB. Names must be unique and cannot reuse Home, Inbox, Later or Books; leading and trailing spaces and case do not distinguish names.
