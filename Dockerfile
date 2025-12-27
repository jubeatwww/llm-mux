# Stage 1: Build Rust binary
FROM rust:1.92-alpine AS builder

RUN apk add --no-cache musl-dev

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

# Stage 2: Runtime with Node.js CLI tools
FROM node:24-alpine

RUN npm install -g @google/gemini-cli @anthropic-ai/claude-code @openai/codex

WORKDIR /app

COPY --from=builder /app/target/release/llm-mux /usr/local/bin/llm-mux
COPY config.toml ./config.toml

ENV LLM_MUX_CONFIG=/app/config.toml

EXPOSE 3000

CMD ["llm-mux"]