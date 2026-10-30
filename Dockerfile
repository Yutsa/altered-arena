# Altered Arena server image. Build from the parent folder of altered-arena (it holds AlteredBGA,
# whose card art is copied in):  docker build -t altered-arena -f altered-arena/Dockerfile .
FROM rust:1-slim AS server
WORKDIR /src
COPY altered-arena/Cargo.toml altered-arena/Cargo.lock ./
COPY altered-arena/rules rules
COPY altered-arena/goldens goldens
COPY altered-arena/ai ai
COPY altered-arena/replay replay
COPY altered-arena/imitate imitate
COPY altered-arena/arena-server arena-server
COPY altered-arena/loadtest loadtest
RUN cargo build --release -p arena-server

FROM node:22-slim AS client
WORKDIR /src
COPY altered-arena/protocol protocol
COPY altered-arena/client client
RUN cd client && npm ci && npm run build

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=server /src/target/release/arena-server /app/arena-server
COPY --from=client /src/client/dist /app/client
COPY altered-arena/protocol/data /app/protocol
COPY AlteredBGA/img/illustrations /app/art
ENV PORT=8080 CLIENT_DIR=/app/client ART_DIR=/app/art PROTOCOL_DIR=/app/protocol DATA_DIR=/data MIMALLOC_PURGE_DELAY=0
VOLUME /data
EXPOSE 8080
CMD ["/app/arena-server"]
