# rustfolio

A personal engineering blog for [@nayandas69](https://github.com/nayandas69) — a static
site **generated in Rust**, styled in **SCSS**, written in **Markdown**, deployed to
**Vercel**.

Live at **https://rustfolio.vercel.app**

## Stack

| Layer | Choice |
|---|---|
| Static site generator | **Rust** (pinned in [`rust-toolchain.toml`](rust-toolchain.toml)) |
| Markdown | [`pulldown-cmark`](https://github.com/pulldown-cmark/pulldown-cmark) |
| SCSS (minified) | [`grass`](https://github.com/connorskees/grass) |
| Syntax highlighting | [`syntect`](https://github.com/trishume/syntect) |
| Templates | [`minijinja`](https://github.com/mitsuhiko/minijinja) |
| Frontend | Semantic HTML5, SCSS, a few KB of vanilla JS |
| Hosting | Vercel — fully static output, no server runtime |

## Deploying to Vercel

| Vercel setting | What to choose |
|---|---|
| **Framework Preset** | **Other** |
| **Root Directory** | Leave as-is (the repository root) |
| **Build Command** | See command below |
| **Output Directory** | **`dist`** |
| **Install Command** | Leave empty |

Build Command (installs the Rust toolchain if the build image has none, then builds):

```bash
cargo --version >/dev/null 2>&1 || { curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal; . "$HOME/.cargo/env"; }; cargo run -- build
```

## Local development

Requirements: a stable Rust toolchain (the exact version is pinned in
[`rust-toolchain.toml`](rust-toolchain.toml), applied automatically by rustup).

```bash
git clone https://github.com/nayandas69/rustfolio
cd rustfolio

cargo run -- build          # build the site into dist/
cargo run -- serve          # serve it on http://localhost:8080
cargo run -- check          # validate config, content, SCSS, templates
cargo run -- verify --output dist   # verify the built site (links, feeds, artifacts)
```

| Command | What it does |
|---|---|
| `cargo run -- build` | Full production build into `dist/` (atomic swap) |
| `cargo run -- build --include-drafts` | Local build that includes drafts |
| `cargo run -- check` | Validate config, frontmatter, content, SCSS, templates |
| `cargo run -- scss` | Compile all SCSS entrypoints in strict mode |
| `cargo run -- verify --output dist` | Verify a built site: artifacts, links, RSS, sitemap |
| `cargo run -- serve --port 8080` | Serve a built site locally |
| `cargo fmt --all` / `cargo clippy -- -D warnings` / `cargo test` | Format / lint / test |

## Writing articles

Articles live in `content/posts/<slug>.md` — the filename is the URL slug
(kebab-case). Frontmatter is strict: unknown keys are errors, dates must be
`YYYY-MM-DD`, `accent` must be `#RRGGBB`.

```yaml
---
title: "Building a Static Site Generator in Rust"
description: "An engineering-focused guide."
date: 2026-10-20
tags: [rust, systems-programming]
featured: true
theme: paper        # default | dark | paper | minimal
draft: false        # true = validated but excluded from production builds
---
```

- Internal links and asset references are verified at build time; a broken link fails
  the build.
- Relative image paths resolve against `static/` — `![alt](assets/x.svg)` serves from
  `/assets/x.svg`.
- Publishing a draft is just flipping `draft: false` and pushing.

## Site configuration

All site-wide metadata — URL, title, description, author, social links — lives in
[`rustfolio.toml`](rustfolio.toml). After the first deployment, make sure `site.url`
matches the real production URL.

## License

[MIT](LICENSE) © 2026 nayandas69
