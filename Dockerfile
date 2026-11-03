# Music Showcase server image (linux/arm64 for Fargate Graviton, builds natively on Apple silicon / arm64 CI).
#   docker build --platform linux/arm64 -t music-server .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY server/Cargo.toml server/Cargo.lock ./
COPY server/src ./src
RUN cargo build --release --locked --features aws \
 && install -d -o 65532 -g 65532 -m 0700 /data-skel

# Distroless: no shell, no package manager; runs as the built-in nonroot user (65532).
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/music-server /music-server
COPY --from=build --chown=65532:65532 /data-skel /data
ENV MUSIC_BIND=0.0.0.0 MUSIC_PORT=8080 MUSIC_DATA_DIR=/data
USER 65532:65532
EXPOSE 8080
HEALTHCHECK --interval=15s --timeout=5s --retries=3 CMD ["/music-server", "healthcheck"]
ENTRYPOINT ["/music-server"]
