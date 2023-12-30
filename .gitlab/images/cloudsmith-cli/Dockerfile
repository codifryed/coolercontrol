FROM ubuntu:focal

LABEL maintainer=codifryed

ENV DEBIAN_FRONTEND=noninteractive
# Use C.UTF-8 locale to avoid issues with ASCII encoding
ENV LC_ALL C.UTF-8
ENV LANG C.UTF-8
ENV CI true

RUN apt update && apt install -y  \
    curl \
    git \
    python3 \
    python3-pip \
    # python symlink \
    python-is-python3 && \
    pip3 install --upgrade pip && \
    pip install --upgrade cloudsmith-cli && \
    apt-get -y autoclean
