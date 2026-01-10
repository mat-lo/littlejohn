# littlejohn

Native terminal UI for torrent search with Real-Debrid integration, written in Rust.

## Features

- Fast, native terminal interface built with ratatui
- Search multiple torrent sites in parallel (standalone, no backend required)
- Real-Debrid integration for premium downloads
- File picker for multi-file torrents
- Built-in download manager with progress tracking
- Vim-style keybindings (j/k navigation)

## Building

```bash
cargo build --release
```

The binary will be at `target/release/littlejohn`.

## Configuration

Set your Real-Debrid API token in `.env` (in the project root or parent directory):

```
RD_API_TOKEN=your_token_here
```

Get your token from: https://real-debrid.com/apitoken

### Download Directory

Optionally set a custom download directory:

```
DOWNLOAD_DIR=/path/to/downloads
```

If not set, files are saved to your system's default Downloads folder.

### Firecrawl (Optional)

Some sites (Il Corsaro Nero, YTS) use anti-bot protection. Firecrawl helps bypass this:

```
FIRECRAWL_API_KEY=your_key_here
```

Get your key from: https://firecrawl.dev

Without this, Il Corsaro Nero won't work and YTS may be less reliable.

## Usage

```bash
cargo run --release
```

Or run the built binary directly:

```bash
./target/release/littlejohn
```

## Keybindings

### Search Screen

| Key     | Action                       |
| ------- | ---------------------------- |
| `Enter` | Search / Process magnet link |
| `s`     | Select sources               |
| `d`     | View downloads               |
| `Esc`   | Quit                         |

### Results Screen

| Key          | Action         |
| ------------ | -------------- |
| `j` / `Down` | Move down      |
| `k` / `Up`   | Move up        |
| `Enter`      | Select torrent |
| `n`          | Next page      |
| `p`          | Previous page  |
| `s`          | Select sources |
| `d`          | View downloads |
| `/`          | Back to search |
| `q`          | Quit           |

### File Select Screen

| Key          | Action                |
| ------------ | --------------------- |
| `j` / `Down` | Move down             |
| `k` / `Up`   | Move up               |
| `Space`      | Toggle file selection |
| `a`          | Toggle all files      |
| `Enter`      | Confirm selection     |
| `Esc`        | Cancel                |

### Downloads Screen

| Key          | Action                  |
| ------------ | ----------------------- |
| `j` / `Down` | Move down               |
| `k` / `Up`   | Move up                 |
| `s`          | Start selected download |
| `S`          | Start all downloads     |
| `c`          | Cancel selected         |
| `C`          | Cancel all              |
| `x`          | Clear completed         |
| `Esc`        | Back                    |

## Supported Sites

- 1337x
- The Pirate Bay (TPB)
- BitSearch
- YTS
- Il Corsaro Nero

## Dependencies

- [ratatui](https://github.com/ratatui/ratatui) - Terminal UI framework
- [tokio](https://tokio.rs) - Async runtime
- [reqwest](https://github.com/seanmonstar/reqwest) - HTTP client
- [scraper](https://github.com/causal-agent/scraper) - HTML parsing

## Architecture

```
src/
├── main.rs         # Application state, event loop, async messaging
├── ui.rs           # Terminal UI rendering (ratatui)
├── realdebrid.rs   # Real-Debrid API client
├── lib.rs          # Shared types and utilities
└── scrapers/       # Site-specific scrapers
    ├── mod.rs      # Scraper registry and common types
    ├── x1337.rs    # 1337x scraper
    ├── tpb.rs      # TPB scraper
    ├── bitsearch.rs
    ├── yts.rs
    └── ilcorsaronero.rs
```

The app uses an async message-passing architecture:

- Main loop handles keyboard events and renders UI
- Background tasks handle scraping and API calls
- Messages are sent back via tokio channels
