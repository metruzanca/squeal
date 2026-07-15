# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-07-15

### Added

- Fuzzy finder — press `Ctrl+P` / `Ctrl+K` to jump to any table or saved query.
- Peak view — press `Space` on a focused row to see full column values with type, primary key, and foreign key metadata.
- Schema groups in sidebar with expand/collapse for PostgreSQL schemas.
- Random adjective-animal names for database connections (e.g. `brave_penguin`, `quick_fox`), deterministically derived from the connection string.
- Query errors now display the actual database error message (red "Error" block in results) instead of a generic table view.
- Queries are persisted to disk by default in `.squeal/<db_name>/queries/` and survive application restarts.

### Fixed

- Recent database entries store absolute paths for reliable reopening from any directory.
- Demo mode (`--demo`) no longer attempts to persist queries to disk.
- Saved queries are now isolated per database, preventing query leakage between different connections.

### Changed

- Queries directory structure changed from `.squeal/queries/` to `.squeal/<db_name>/queries/`.

## [0.2.0] - 2026-06-15

### Added

- PostgreSQL driver support with auto-detection from `postgres://` URLs.
- Recent databases screen shown when launched without arguments.
- Custom queries in views sidebar — write, save, and browse ad-hoc SQL.
- Type-aware filter operators with numeric comparisons and not equals.
- Auto-refresh table data every 5 seconds (manual refresh with `r`).
- SQL syntax highlighting in custom query text area.
- Arrow symbols in keybind bar replacing vim bind labels.

### Fixed

- Pressing Enter on a row no longer panics when opening details modal on PostgreSQL.
- Foreign key query on PostgreSQL rewritten with `UNNEST` for correctness.
- Row details modal now shows correctly when table has no foreign keys.
- Auto-refresh now works while previewing a table in the sidebar.
- Ellipsis truncation removed from column headers; sort arrow spacing fixed.
- Custom queries wrapped in read-only transactions to prevent accidental writes.

## [0.1.0] - 2026-06-12

### Added

- Browse any SQLite database with a split-pane TUI interface.
- Vim-style navigation (arrow keys also supported).
- Lazy row loading — loads 100 rows at a time, fetches more on demand.
- Column filtering by exact match or substring.
- Column sorting in ascending or descending order.
- Foreign key record view — press `Enter` on a row to see related records.
- Help overlay — press `?` to see all available keybindings.
- `cargo binstall` support via prebuilt GitHub release binaries.

[Unreleased]: https://github.com/metruzanca/squeal/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/metruzanca/squeal/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/metruzanca/squeal/releases/tag/v0.2.0
[0.1.0]: https://github.com/metruzanca/squeal/releases/tag/v0.1.0
