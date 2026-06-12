# squeal

A lightweight TUI SQLite database viewer built in Rust.

`squeal` lets you open any SQLite database file and browse its tables directly in the terminal. It features a split-pane interface: a table list on the left and the selected table's data on the right. Navigation is vim-inspired, and large tables are lazily loaded so you can inspect databases of any size without freezing your terminal.

[![asciinema](https://asciinema.org/a/Th7yS2UE30KD1IOc.svg)](https://asciinema.org/a/Th7yS2UE30KD1IOc)

## Features

- **Browse any SQLite database** — open `.db` or `.sqlite` files instantly
- **Split-pane layout** — table list on the left, data on the right
- **Vim-style navigation** — Supports vim movement natively, as well as arrow keys.
- **Lazy row loading** — loads 100 rows at a time, fetches more on demand as you scroll
- **Column filtering** — filter by exact match or substring contains per column
- **Column sorting** — sort any column ascending or descending
- **Foreign key record view** — press `Enter` on a row to view related records from referenced tables
- **Help overlay** — press `?` anytime to see all available keybindings

## Installation

```bash
# Install Binary directly via github
cargo binstall --git https://github.com/metruzanca/squeal --target squeal
# Install from github via local build
cargo install --git https://github.com/metruzanca/squeal
```

## Usage

Open a database file:

```bash
squeal my-database.db
squeal --demo # to preview with dummy data
```

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

<details>
<summary>Why is it called squeal?</summary>

Based on how [ThePrimeagen](https://x.com/ThePrimeagen/status/1703196414153511205) [jokingly](https://x.com/theprimeagen/status/1437476573863677955) pronounces SQL.

</details>
