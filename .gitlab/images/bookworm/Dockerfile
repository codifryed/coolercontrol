FROM debian:bookworm

LABEL maintainer=codifryed
# Image for building non-Ubuntu debian packages (debian has different dependency versions)

ENV DEBIAN_FRONTEND=noninteractive
# Use C.UTF-8 locale to avoid issues with ASCII encoding
ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8
ENV CI=true
# current bookworm rust version is 1.63.0 - a bit old and possiblity not compatible with some crates
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    RUST_VERSION=1.82.0
# bookworm has modern python dependencies - no issue.
# bookworm has v18 nodejs - no issue.

RUN apt update && apt install -y --no-install-recommends \
    # build-deps:
    dh-python \
    pybuild-plugin-pyproject \
    python3-all \
    python3-setuptools \
    python3-build \
    liquidctl \
    python3-setproctitle \
    python3-fastapi \
    python3-uvicorn \
    nodejs \
    npm \
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
    cargo \
    # rustup:
    curl \
    ca-certificates \
    debhelper

# rust toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --profile minimal --default-toolchain $RUST_VERSION && \
    chmod -R a+w $RUSTUP_HOME $CARGO_HOME

RUN apt-get -y autoclean
