FROM amd64/debian:bullseye

LABEL maintainer=codifryed
# Image for building and testing AppImages

ENV DEBIAN_FRONTEND=noninteractive
# Use C.UTF-8 locale to avoid issues with ASCII encoding
ENV LC_ALL C.UTF-8
ENV LANG C.UTF-8
ENV CI true
ENV PYTHONFAULTHANDLER=1 \
    PYTHONUNBUFFERED=1 \
    # pip
    PIP_DISABLE_PIP_VERSION_CHECK=on \
    PIP_DEFAULT_TIMEOUT=100
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    RUST_VERSION=1.75.0
ENV NODE_MAJOR=18

RUN apt update && apt install -y --no-install-recommends \
    # build essentials:
    software-properties-common \
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
    # npm:
    ca-certificates \
    gnupg \
    # tauri:
    # already used above:  build-essential \
    # already used above: curl \
    wget \
    file \
    libwebkit2gtk-4.0-dev \
    libssl-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    # python symlink
    python-is-python3 \
    # debian package helper
    debhelper

# python 3.11.8 used for AppImages (Nuitka does not yet support 3.12)
RUN wget https://www.python.org/ftp/python/3.11.8/Python-3.11.8.tgz && \
    tar -xf Python-3.11.*.tgz && \
    cd Python-3.11.*/ && \
    ./configure --enable-optimizations && \
    make -j$(nproc) && \
    make install

# rust toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --profile minimal --default-toolchain $RUST_VERSION && \
    chmod -R a+w $RUSTUP_HOME $CARGO_HOME

RUN mkdir -p /etc/apt/keyrings && \
    curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg && \
    echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_$NODE_MAJOR.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list && \
    apt-get update && apt-get install -y --no-install-recommends nodejs

RUN pip3 install --upgrade pip

RUN apt-get -y autoclean
