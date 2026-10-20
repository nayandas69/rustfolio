---
title: "About"
description: "Who writes here, what this site is built with, and how it is maintained."
---

## Hello

I'm a software engineer writing about Rust, systems programming, and the craft
of building small, dependable tools for the web.

This site is where I think out loud: long-form engineering notes, worked
experiments, and the occasional strong opinion. I care about software that
fails loudly, builds reproducibly, and stays maintainable by one person.

## How this site is made

Every page here is generated from Markdown by a small static site generator
written in Rust:

- **Content** lives in [`content/`](https://github.com/nayandas69/rustfolio) as plain Markdown with YAML frontmatter.
- **Styling** is authored exclusively in SCSS and compiled at build time.
- **The build** is deterministic, validates its own output, and publishes atomically.
- **The frontend** is semantic HTML with a few kilobytes of progressive enhancement — no framework, no tracking, no external fonts.

Publishing an article is a commit and a push; everything else is automated.

## Elsewhere

You can find my code on [GitHub](https://github.com/nayandas69). If you enjoy
long-form technical writing, subscribe via [RSS](/rss.xml) — it is the one
channel that never plays games with reach.
