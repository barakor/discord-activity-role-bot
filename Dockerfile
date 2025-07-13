FROM rust:1.88

RUN apt-get update && apt-get install -y \
    cmake \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*


COPY ./ /app


EXPOSE 443
EXPOSE 80

CMD cargo run
