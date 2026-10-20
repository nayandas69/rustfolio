---
title: "Notes on Error Handling Ergonomics"
description: "Working draft: comparing error strategies across the standard library, thiserror and anyhow."
date: 2026-10-18
draft: true
tags:
  - rust
toc: true
---

This is a **draft** — it is excluded from production builds by `draft: true` in
the frontmatter, and the build fails if anything about it is invalid. Drafts
are fully validated so you can park half-written work in the repository.

Outline so far:

- `Result<T, E>` and the two error philosophies: typed domain errors vs. rich context.
- When to reach for `thiserror` (library boundaries) versus `anyhow` (application edges).
- Error reports that point at files and line numbers.

```rust
#[derive(Debug, thiserror::Error)]
pub enum SiteError {
    #[error("in `{file}`, line {line}: unsafe link `{target}`")]
    UnsafeLink {
        file: String,
        line: usize,
        target: String,
    },
}
```

Publishing this post is a one-line change: set `draft: false` and push.
