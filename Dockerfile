FROM rust:1.88

RUN apt-get update && apt-get install -y \
    cmake \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs

# 3. Build dependencies only
RUN cargo build --release
RUN rm -rf src

# Now copy the actual source code
COPY . .
RUN cargo build --release


EXPOSE 443
EXPOSE 80

CMD cargo run --release
