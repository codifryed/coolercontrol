FROM opensuse/tumbleweed

MAINTAINER codifryed

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
    POETRY_VERSION=1.5.1
ENV PATH="/root/.local/bin:$POETRY_HOME/bin:$PATH"
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    RUST_VERSION=1.71.0 \
    LD_LIBRARY_PATH=/usr/local/lib

RUN zypper install -y \
    # rpm building deps \
    rpmbuild \
    rpmdevtools \
    systemd-rpm-macros libappstream-glib8  \
    curl wget \
    dbus-1 \
    tar \
    # for standard appstream checks
    desktop-file-utils AppStream \
    && \
    rpmdev-setuptree
