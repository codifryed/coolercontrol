FROM ubuntu:20.04

MAINTAINER codifryed

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/root/.local/bin:$PATH"
# Use C.UTF-8 locale to avoid issues with ASCII encoding
ENV LC_ALL C.UTF-8
ENV LANG C.UTF-8
ENV CI true


RUN apt-get update && apt-get install -y software-properties-common && \
    add-apt-repository ppa:deadsnakes/ppa -y && \
    apt-get update && apt-get install -y \
        chrpath gdb ccache libfuse2 patchelf \
        desktop-file-utils appstream-util \
        python3.9 python3-pip \
        python3.9-dev python3.9-venv \
        libusb-1.0-0 \
        curl \
        git \
        build-essential \
        dbus \
        xvfb \
        libglu1-mesa-dev \
        libx11-xcb-dev \
        libxkbcommon-x11-0 \
        '^libxcb*' && \
    pip3 install --upgrade pip && \
    apt-get -y autoclean

RUN curl -sSL --output /tmp/install-poetry.py https://install.python-poetry.org && \
    python3.9 /tmp/install-poetry.py && \
    poetry config virtualenvs.create true

COPY build.sh /root/

RUN mkdir -p /app/coolero && \
    chmod +x /root/build.sh && \
    mkdir /root/.gnupg && \
    chmod 700 /root/.gnupg

WORKDIR /app/coolero
CMD ["/root/build.sh"]
