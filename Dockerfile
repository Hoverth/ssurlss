FROM rust AS builder
WORKDIR /usr/src/ssurlss
COPY . .
RUN cargo install --path . --root /usr/local/cargo/

FROM debian
#RUN apt-get update && apt-get install -y extra-runtime-dependencies && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/ssurlss /ssurlss
RUN chmod +x /ssurlss
WORKDIR /
CMD ["/ssurlss"]
