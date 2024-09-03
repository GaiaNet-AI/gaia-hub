FROM rust:1.80-slim-bullseye as builder

RUN apt-get update
RUN apt-get install libssl-dev pkg-config libsqlite3-dev -y

COPY . /project
WORKDIR /project

RUN cargo build --release



FROM debian:bullseye-slim

COPY --from=builder /project/target/release/gaia-hub /app/gaia-hub

WORKDIR /app/
RUN apt-get update
RUN apt-get install libsqlite3-dev -y
CMD ["./gaia-hub"]
