FROM ubuntu:jammy

MAINTAINER codifryed

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
    PIP_DEFAULT_TIMEOUT=100 \
    # poetry
    POETRY_NO_INTERACTION=1 \
    POETRY_HOME="/opt/poetry" \
    POETRY_VERSION=1.7.1
ENV PATH="/root/.local/bin:$POETRY_HOME/bin:$PATH"
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    RUST_VERSION=1.74.0
ENV NODE_MAJOR=18

RUN apt update && apt install -y software-properties-common \
    # build essentials:
    curl \
    git \
    build-essential \
    dbus \
    # nuitka deps for building:
    chrpath gdb ccache libfuse2 patchelf \
    # for standard appstream checks:
    desktop-file-utils appstream-util \
    # python
    zlib1g-dev libncurses5-dev libgdbm-dev libnss3-dev libssl-dev libreadline-dev libffi-dev libsqlite3-dev wget libbz2-dev \
    python3-pip \
    # need base usb package for liquidctl
    libusb-1.0-0 \
    # Qt testing deps:
    libsm6 libxext6 \
    libgl1-mesa-glx \
    libegl1-mesa \
    libfontconfig \
    libxkbcommon-x11-0 \
    # rust daemon deps:
    # libssl-dev \
    pkg-config \
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
    # python symlink
    python-is-python3 \
    # debian package helper
    debhelper && \
    pip3 install --upgrade pip && \
    apt-get -y autoclean

# python 3.11.4
RUN wget https://www.python.org/ftp/python/3.11.4/Python-3.11.4.tgz && \
    tar -xf Python-3.11.*.tgz && \
    cd Python-3.11.*/ && \
    ./configure --enable-optimizations && \
    make -j$(nproc) && \
    make install && \
    python3.11 --version

# poetry
RUN curl -sSL --output /tmp/install-poetry.py https://install.python-poetry.org && \
    python3.11 /tmp/install-poetry.py

# rust toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --profile minimal --default-toolchain $RUST_VERSION && \
    chmod -R a+w $RUSTUP_HOME $CARGO_HOME

# for our conversion to gitlib junit test results
RUN cargo install cargo2junit

RUN mkdir -p /etc/apt/keyrings && \
    curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg && \
    echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_$NODE_MAJOR.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list && \
    apt-get update && apt-get install nodejs -y
