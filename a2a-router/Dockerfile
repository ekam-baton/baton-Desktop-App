FROM rust:1.80-alpine AS builder

WORKDIR /usr/src/app
RUN apk add --no-cache musl-dev

# Copy workspace
COPY . .

# Build only a2a-router
RUN cargo build --release -p a2a-router

FROM alpine:latest
WORKDIR /app
COPY --from=builder /usr/src/app/target/release/a2a-router /usr/local/bin/

CMD ["a2a-router"]
