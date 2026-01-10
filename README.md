# Goering

Torrent search API aggregator with Real-Debrid integration.

## Features

- Search multiple torrent sites simultaneously (1337x, Nyaa.si, YTS, TPB, etc.)
- FastAPI REST API with unified JSON responses
- Interactive CLI with Real-Debrid download integration
- Results sorted by seeders with source priority
- Background download manager with progress tracking

## Installation

```bash
# Install dependencies
uv sync
```

## Configuration

Set your Real-Debrid API token in `.env`:

```
RD_API_TOKEN=your_token_here
```

Get your token from: https://real-debrid.com/apitoken

## Usage

### Interactive CLI

```bash
uv run python -m goering.cli
```

Features:
- Search torrents with arrow key navigation
- Select/filter torrent sources
- Multi-file torrent support with file picker
- Foreground or background downloads
- Download progress tracking

### API Server

```bash
uv run uvicorn goering.app:app --host 0.0.0.0 --port 8000 --reload
```

#### Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /` | API info and available sites |
| `GET /api/{site}/{query}` | Search a specific site |
| `GET /api/{site}/{query}/{page}` | Search with pagination |
| `GET /api/all/{query}` | Search all sites in parallel |

#### Example

```bash
curl "http://localhost:8000/api/all/ubuntu"
```

## Supported Sites

- 1337x
- Nyaa.si
- YTS
- The Pirate Bay (TPB)
- BitSearch
- GloDLS
- Il Corsaro Nero
- ExtTo

## Project Structure

```
goering/
├── app.py          # FastAPI application
├── cli.py          # Interactive terminal client
├── realdebrid.py   # Real-Debrid API client
├── download.py     # Download manager
└── scrapers/       # Site-specific scrapers
    ├── base.py     # Shared fetch utilities
    ├── x1337.py    # 1337x scraper
    ├── nyaasi.py   # Nyaa.si scraper
    └── ...
```

## Related

See [goering-tui](./goering-tui/) for a native Rust TUI alternative.
