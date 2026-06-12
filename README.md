# squeal

A lightweight TUI SQLite database viewer built in Rust.

`squeal` lets you open any SQLite database file and browse its tables directly in the terminal. It features a split-pane interface: a table list on the left and the selected table's data on the right. Navigation is vim-inspired, and large tables are lazily loaded so you can inspect databases of any size without freezing your terminal.

![Rust](https://img.shields.io/badge/rust-2024%20edition-orange?logo=rust)

## Features

- **Browse any SQLite database** — open `.db` or `.sqlite` files instantly
- **Split-pane layout** — table list on the left, data on the right
- **Vim-style navigation** — `j`/`k` to move, `h`/`l` to focus/unfocus the data pane
- **Lazy row loading** — loads 100 rows at a time, fetches more on demand as you scroll
- **Horizontal scrolling** — wide tables with many columns are automatically scrollable
- **In-memory demo mode** — try it out without a database file using `--demo`
- **All SQLite types** — correctly handles `NULL`, `INTEGER`, `REAL`, `TEXT`, and `BLOB`

## Installation

```bash
cargo install --git https://github.com/metruzanca/squeal
# or
cargo install --path .
```

## Usage

Open a database file:

```bash
squeal my-database.db
```

## Controls

| Key | Action |
| --- | --- |
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `l` / `→` | Focus the data pane |
| `h` / `←` | Unfocus the data pane |
| `q` | Quit |

When the data pane is focused, `j`/`k` scroll through rows and `h`/`l` scroll horizontally across wide tables.

## Tech Stack

- [ratatui](https://github.com/ratatui/ratatui) — terminal UI framework
- [rusqlite](https://github.com/rusqlite/rusqlite) — SQLite bindings for Rust
- [crossterm](https://github.com/crossterm-rs/crossterm) — cross-platform terminal input
- [clap](https://github.com/clap-rs/clap) — CLI argument parsing

<details>
<summary>How AI was used</summary>

This project was written entirely through agentic coding. I ([@metruzanca](https://github.com/metruzanca)) didn't write a single line of code—everything was done through prompts with Kimi K2.5 via Opencode. I've been writing code professionally since late 2019 and coding even longer than that, so while I didn't write the code, I wasn't flying blind when steering the agent in the right direction.

I'm currently using `squeal` in my own dev environment and it's working well. You're free to use it as-is or modify it to suit your needs.

</details>
