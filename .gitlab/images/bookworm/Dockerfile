FROM debian:bookworm

LABEL maintainer=codifryed
# Image for building non-Ubuntu debian packages (debian has different dependency versions)

ENV DEBIAN_FRONTEND=noninteractive
# Use C.UTF-8 locale to avoid issues with ASCII encoding
ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8
ENV CI=true
# The default (compio-rt) build now needs Rust 1.88.  1.85 only covers the
# `--no-default-features` build. Using the same version as Ubuntu Jammy
# Whenever we build a Trixie build, backports has 1.91
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    RUST_VERSION=1.91.0
# bookworm used node 18
ENV NODE_MAJOR=22
# bookworm has modern python dependencies - no issue.

RUN apt update && apt install -y --no-install-recommends \
    # build-deps:
    # Recommends:
    liquidctl \
    lm-sensors \
    #nodejs \
    #npm \
    # nodejs from nodesource:
    ca-certificates \
    gnupg \
    # qt:
    build-essential \
    cmake \
    xvfb \
    libgl1-mesa-dev \
    libqt6opengl6-dev \
    qt6-base-dev \
    qt6-webengine-dev \
    qt6-webengine-dev-tools \
    libdrm-dev \
    # Bookworm's newest package version is 1.85
    #cargo-web \
    # external libs:
    curl \
    ca-certificates \
    debhelper

# rust toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --profile minimal --default-toolchain $RUST_VERSION && \
    chmod -R a+w $RUSTUP_HOME $CARGO_HOME
# cargo-web does not need a symlink for cargo, unlike the cargo-* packages (see ubuntu)

RUN mkdir -p /etc/apt/keyrings && \
    curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg && \
    echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_$NODE_MAJOR.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list && \
    apt-get update && apt-get install -y --no-install-recommends nodejs

RUN apt-get -y autoclean
