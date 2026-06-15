# squeal

A lightweight TUI database viewer for SQLite and PostgreSQL, built in Rust.

`squeal` lets you open any SQLite or PostgreSQL database and browse its tables directly in the terminal. It features a split-pane interface. Navigation is intuitive and easy, and large tables are lazily loaded.

[![asciinema](.github/squeal-preview.png)](https://asciinema.org/a/Th7yS2UE30KD1IOc)

## Features

- **SQLite & PostgreSQL** — auto-detects connection type from the URL
- **Recent databases** — startup screen with recently opened databases
- **Split-pane layout** — table list on the left, data on the right
- **Vim-style navigation** — supports vim movement keys natively, as well as arrow keys
- **Lazy row loading** — loads 100 rows at a time, fetches more on demand as you scroll
- **Column filtering** — type-aware operators: exact match, substring, numeric comparisons
- **Column sorting** — sort any column ascending or descending
- **Foreign key record view** — press `Enter` on a row to view related records from referenced tables
- **Custom queries** — write, save, and browse ad-hoc SQL queries
- **Auto-refresh** — table data refreshes every 5 seconds (manual refresh with `r`)
- **Help overlay** — press `?` anytime to see all available keybindings

## Installation

```bash
# Install via cargo binstall (picks up prebuilt binaries from GitHub releases)
cargo binstall squeal
# Install from source via GitHub
cargo install --git https://github.com/metruzanca/squeal
# Oneliner for servers
curl -sL https://github.com/metruzanca/squeal/releases/latest/download/squeal-x86_64-unknown-linux-musl.tar.gz | tar -xz && ./squeal
```

## Usage

Open a database:

```bash
squeal my-database.db                # SQLite
squeal postgres://user:pass@host/db  # PostgreSQL
squeal --demo                        # preview with dummy data
```

## Tech Stack

- [ratatui](https://github.com/ratatui/ratatui) — terminal UI framework
- [rusqlite](https://github.com/rusqlite/rusqlite) — SQLite bindings for Rust
- [postgres](https://github.com/sfackler/rust-postgres) — PostgreSQL client for Rust
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
