FROM ubuntu:jammy

LABEL maintainer=codifryed
# Image to build Ubuntu packages

ENV DEBIAN_FRONTEND=noninteractive
# Use C.UTF-8 locale to avoid issues with ASCII encoding
ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8
ENV CI=true

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    RUST_VERSION=1.90.0
ENV PATH=/root/.cargo/bin:$PATH

ENV NODE_MAJOR=22

RUN apt update && apt install -y --no-install-recommends \
    # build-deps:
    # Recommends:
    liquidctl \
    lm-sensors \
    # nodejs \
    # npm \
    # qt:
    build-essential \
    cmake \
    xvfb \
    libgl1-mesa-dev \
    libqt6opengl6-dev \
    qt6-base-dev \
    qt6-webengine-dev \
    qt6-webengine-dev-tools \
    # coolercontrold
    libdrm-dev \
    cargo \
    protobuf-compiler \
    # nodesource:
    curl \
    ca-certificates \
    gnupg \
    dh-cmake \
    debhelper

# rust toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --profile minimal --default-toolchain $RUST_VERSION && \
    chmod -R a+w $RUSTUP_HOME $CARGO_HOME

RUN mkdir -p /etc/apt/keyrings && \
    curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg && \
    echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_$NODE_MAJOR.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list && \
    apt-get update && apt-get install -y --no-install-recommends nodejs

RUN apt-get -y autoclean
