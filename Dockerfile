FROM rust:alpine AS builder
RUN apk add musl-dev
WORKDIR /usr/src/ssurlss
COPY . .
RUN cargo install --path . --root /usr/local/cargo/

FROM alpine
#RUN apt-get update && apt-get install -y extra-runtime-dependencies && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/ssurlss /ssurlss
RUN chmod +x /ssurlss
WORKDIR /
RUN touch /ssurlss.toml
CMD ["/ssurlss"]
