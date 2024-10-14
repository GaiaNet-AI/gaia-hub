FROM rust:1.80-slim-bullseye as builder

ARG HUB_DB=sqlite

RUN apt-get update
RUN apt-get install libssl-dev pkg-config libsqlite3-dev default-libmysqlclient-dev -y

COPY . /project
WORKDIR /project

RUN cargo build --no-default-features --features ${HUB_DB} --release



FROM debian:bullseye-slim

COPY --from=builder /project/target/release/gaia-hub /app/gaia-hub

WORKDIR /app/
RUN apt-get update
RUN apt-get install libsqlite3-dev default-libmysqlclient-dev -y
CMD ["./gaia-hub"]
