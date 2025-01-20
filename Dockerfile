FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl-dev pkg-config libudev-dev
RUN apt install -y build-essential curl
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustc --version && cargo --version

# Создание рабочей директории
WORKDIR /usr/src/app

COPY . .

RUN cargo build --release

RUN cp ./target/release/aptos_parser .
RUN cargo clean

RUN chmod +x ./aptos_parser

CMD ["./aptos_parser"]

