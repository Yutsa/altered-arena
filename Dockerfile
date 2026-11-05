# Altered Arena server image, built from the repository root:  docker build -t altered-arena .
FROM rust:1-slim AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY rules rules
COPY protocol/data protocol/data
COPY goldens goldens
COPY ai ai
COPY replay replay
COPY imitate imitate
COPY arena-server arena-server
COPY loadtest loadtest
RUN cargo build --release -p arena-server

FROM node:22-slim AS client
WORKDIR /src
COPY protocol protocol
COPY client client
RUN cd client && npm ci && npm run build

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=server /src/target/release/arena-server /app/arena-server
COPY --from=client /src/client/dist /app/client
COPY protocol/data /app/protocol
COPY art /app/art
ENV PORT=8080 CLIENT_DIR=/app/client ART_DIR=/app/art PROTOCOL_DIR=/app/protocol DATA_DIR=/data MIMALLOC_PURGE_DELAY=0
VOLUME /data
EXPOSE 8080
CMD ["/app/arena-server"]
