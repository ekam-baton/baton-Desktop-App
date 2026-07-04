FROM rust:1.80-alpine AS builder

WORKDIR /usr/src/app
RUN apk add --no-cache musl-dev libpq-dev postgresql-dev

# Copy workspace
COPY . .

# Build only mcp-connector
RUN cargo build --release -p mcp-connector

FROM alpine:latest
WORKDIR /app
RUN apk add --no-cache libpq
COPY --from=builder /usr/src/app/target/release/mcp-connector /usr/local/bin/

CMD ["mcp-connector"]
