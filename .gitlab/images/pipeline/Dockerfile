FROM ubuntu:22.04

LABEL maintainer=codifryed

ENV DEBIAN_FRONTEND=noninteractive
# Use C.UTF-8 locale to avoid issues with ASCII encoding
ENV LC_ALL C.UTF-8
ENV LANG C.UTF-8
ENV CI true
ENV PYTHONFAULTHANDLER=1 \
    PYTHONUNBUFFERED=1 \
    # pip
    PIP_NO_CACHE_DIR=off \
    PIP_DISABLE_PIP_VERSION_CHECK=on \
    PIP_DEFAULT_TIMEOUT=100
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    RUST_VERSION=1.74.0
ENV NODE_MAJOR=18

RUN apt-get update && \
    apt-get install -y software-properties-common && \
    apt-get install -y \
        # for standard appstream checks:
        desktop-file-utils appstream-util \
        # python
        python3-pip \
        python3-venv \
        # need base usb package for liquidctl
        libusb-1.0-0 \
        # build essentials:
        curl \
        git \
        build-essential \
        dbus \
        # npm:
        ca-certificates \
        gnupg \
        # tauri:
        libwebkit2gtk-4.0-dev \
        # already used above:  build-essential \
        # already used above: curl \
        wget \
        file \
        libssl-dev \
        libgtk-3-dev \
        libayatana-appindicator3-dev \
        librsvg2-dev \
        # debian package helper \
        debhelper && \
    pip3 install --upgrade pip

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --profile minimal --default-toolchain $RUST_VERSION && \
    chmod -R a+w $RUSTUP_HOME $CARGO_HOME

# for our conversion to gitlib junit test results
RUN cargo install cargo2junit

RUN mkdir -p /etc/apt/keyrings && \
    curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg && \
    echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_$NODE_MAJOR.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list && \
    apt-get update && apt-get install nodejs -y

RUN apt-get -y autoclean
