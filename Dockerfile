FROM rust:1.88

RUN apt-get update && apt-get install -y \
    cmake \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {println!("Should Have Been Deleted");}' > src/main.rs

# 3. Build dependencies only
RUN cargo build --release
RUN rm -rf src target/release/build target/release/discord-activity-role-bot  target/release/discord-activity-role-bot.d

# Now copy the actual source code
COPY . .
RUN cargo build --release && echo "Cache busted at $(date)"


EXPOSE 443
EXPOSE 80

CMD cargo run --release
