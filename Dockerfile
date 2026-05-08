FROM rust:1-bookworm AS builder

WORKDIR /src
COPY . .
RUN cargo build --release -p secure-chat-relay

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /var/lib/secure-chat --create-home --shell /usr/sbin/nologin securechat \
    && install -d -o securechat -g securechat -m 0750 /data /certs

COPY --from=builder /src/target/release/secure-chat-relay /usr/local/bin/secure-chat-relay

USER securechat
WORKDIR /var/lib/secure-chat

ENV SECURE_CHAT_RELAY_HTTP_ADDR=0.0.0.0:8787
ENV SECURE_CHAT_RELAY_DB=/data/relay.sqlite3

VOLUME ["/data", "/certs"]
EXPOSE 8787/tcp 443/tcp 443/udp

CMD ["secure-chat-relay"]
