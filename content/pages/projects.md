---
title: "Projects"
description: "Selected projects and experiments, with short engineering notes on each."
---

Things I've built and what I learned from them. Everything here is open source
unless noted otherwise.

## rustfolio

This very site — a Rust static site generator with SCSS styling, client-side
search, strict validation and a deterministic, atomic build pipeline.

- **Stack:** Rust, pulldown-cmark, grass (SCSS), syntect, minijinja, vanilla JS.
- **What it taught me:** how much reliability you can buy with a strict build.
  Unknown frontmatter keys, broken internal links and invalid SCSS all fail
  the build instead of shipping.
- **Code:** [nayandas69/rustfolio](https://github.com/nayandas69/rustfolio)

## Small systems experiments

A grab-bag of CLI tools and benchmarks I use to test assumptions about
performance, error handling and API design:

- Event-stream transformations with zero-copy parsing.
- Deterministic snapshot testing without a framework.
- Profiling harnesses that produce stable, diffable output.

## Tools I reach for

| Tool | Why |
|---|---|
| `cargo` + `clippy` | the boring pipeline that keeps code honest |
| `hyperfine` | honest command-line benchmarking |
| `rr` / `perf` | when guessing about performance has to stop |

> Tip: the best side projects are tools you use long after you build them.
