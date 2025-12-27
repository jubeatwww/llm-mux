# llm-mux

A unified HTTP API server that dispatches structured output requests to different LLM CLI tools.

## Features

- **Unified API** - Single endpoint for multiple LLM providers (Claude, Codex, Gemini)
- **Structured Output** - JSON schema-based output generation
- **Rate Limiting** - Per-model RPS, RPM, and concurrent request limits
- **Timeout Control** - Configurable timeout per provider/model
- **Auto Model Selection** - Optional model parameter (CLI tools pick the best model)
- **Docker Ready** - Multi-stage build with all CLI tools pre-installed

## Supported Providers

| Provider | CLI Tool | Auto Model |
|----------|----------|------------|
| `claude` | `claude` | Yes |
| `codex` | `codex` | Yes |
| `gemini` | `gemini` | Yes |

## API

### Health Check

```
GET /health
```

Response:
```json
{"status": "ok"}
```

### Generate

```
POST /generate
Content-Type: application/json
```

Request body:
```json
{
  "provider": "claude",
  "model": "sonnet",
  "prompt": "Your prompt here",
  "schema": {
    "type": "object",
    "properties": {
      "message": { "type": "string" }
    },
    "required": ["message"]
  }
}
```

- `provider` (required) - One of: `claude`, `codex`, `gemini`
- `model` (optional) - Model name. If omitted, the CLI tool selects automatically
- `prompt` (required) - The prompt to send
- `schema` (required) - JSON Schema for structured output

Response:
```json
{
  "output": {
    "message": "Hello from the LLM"
  }
}
```

### Error Responses

| Status | Error |
|--------|-------|
| 400 | Provider not found, Model not found, Auto model not supported |
| 429 | Rate limited |
| 504 | Timeout |
| 500 | Provider execution failed, Output parse error |

## Configuration

Create `config.toml`:

```toml
[server]
host = "0.0.0.0"
port = 3000

[[providers]]
name = "claude"
supports_auto_model = true
rps = 1
rpm = 50
concurrent = 1
timeout_secs = 120

[[providers.models]]
name = "sonnet"
rps = 1
rpm = 50
concurrent = 1
timeout_secs = 120

[[providers.models]]
name = "haiku"
rps = 1
rpm = 60
concurrent = 1
timeout_secs = 60
```

### Rate Limit Options

- `rps` - Requests per second
- `rpm` - Requests per minute
- `concurrent` - Maximum concurrent requests
- `timeout_secs` - Request timeout in seconds

Provider-level settings apply when model is not specified (auto mode).

## Docker

### Build and Run

```bash
docker compose up --build
```

### Volume Mounts

The `docker_root` folder maps to `/root` in the container. Place CLI authentication configs here:

```
docker_root/
├── .claude/           # Claude CLI config
├── .gemini/           # Gemini CLI config
└── .codex/            # Codex CLI config
```

### Environment Variables

- `LLM_MUX_CONFIG` - Path to config file (default: `config.toml`)
- `RUST_LOG` - Log level (default: `llm_mux=info`)

## Local Development

### Prerequisites

- Rust 1.70+
- CLI tools installed: `claude`, `codex`, `gemini`

### Build

```bash
cargo build --release
```

### Run

```bash
cargo run
```

### Test

```bash
cargo test
```

## License

MIT