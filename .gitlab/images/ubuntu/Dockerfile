FROM ubuntu:jammy

LABEL maintainer=codifryed
# Image to build Ubuntu packages

ENV DEBIAN_FRONTEND=noninteractive
# Use C.UTF-8 locale to avoid issues with ASCII encoding
ENV LC_ALL C.UTF-8
ENV LANG C.UTF-8
ENV CI true

# Using 22.04 LTS rust version, currently 1.70
# 22.04 comes with nodejs v12, 23.04/Lunar comes with v18.
# todo: update when 24.04 LTS comes out, then we can use the official nodejs package
ENV NODE_MAJOR=18

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
    # nodejs \
    # npm \
    build-essential \
    libwebkit2gtk-4.0-dev \
    libssl-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    cargo \
    # nodesource:
    curl \
    ca-certificates \
    gnupg \
    debhelper

# needed until ubuntu updates to nodejs from v12 to v18:
RUN mkdir -p /etc/apt/keyrings && \
    curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg && \
    echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_$NODE_MAJOR.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list && \
    apt-get update && apt-get install -y --no-install-recommends nodejs

RUN apt-get -y autoclean
