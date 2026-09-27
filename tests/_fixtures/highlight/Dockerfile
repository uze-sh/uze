FROM rust:1.97 AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:stable-slim
COPY --from=build /src/target/release/uze /usr/local/bin/uze
ENTRYPOINT ["uze"]
