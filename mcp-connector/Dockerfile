FROM rust:alpine AS builder

WORKDIR /usr/src/app
RUN apk add --no-cache musl-dev libpq-dev postgresql-dev pkgconf openssl-dev openssl-libs-static

# Copy workspace
COPY . .

# Build only mcp-connector
RUN cargo build --release -p mcp-connector

FROM alpine:latest
WORKDIR /app
RUN apk add --no-cache libpq
COPY --from=builder /usr/src/app/target/release/mcp-connector /usr/local/bin/

CMD ["mcp-connector"]
