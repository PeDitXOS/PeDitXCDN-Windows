# peditx-cdn-client

Tauri-based CDN client that connects to a relay server and manages panel configuration.

## Prerequisites

- Node.js ≥ 18
- Rust toolchain (rustup)
- Tauri CLI (`cargo install tauri-cli` or via npm)

## Setup

```bash
cp .env.example .env
# Edit .env with your relay IP, panel URL, and API key
npm install
```

## Scripts

| Command | Description |
|---------|-------------|
| `npm run dev` | Start Vite dev server |
| `npm run build` | Build frontend |
| `npm run tauri` | Build Tauri desktop app |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RELAY_IP` | IP address of the relay server |
| `PANEL_URL` | URL of the management panel |
| `PANEL_API_KEY` | API key for panel authentication |

## License

MIT
