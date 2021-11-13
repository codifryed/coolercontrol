FROM simbachain/poetry-3.9:2
# base dockerfile: https://github.com/SIMBAChain/poetry/blob/main/3.9/Dockerfile

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    # done in base image: python3.9 python3-pip python3.9-dev build-essential curl \
    # base system:
    ccache \
    dbus \
    # Qt testing deps:
    xvfb \
    libglu1-mesa-dev \
    libx11-xcb-dev \
    libxkbcommon-x11-0 \
    '^libxcb*'

# Use C.UTF-8 locale to avoid issues with ASCII encoding
ENV LC_ALL C.UTF-8
ENV LANG C.UTF-8
ENV CI true
ENV DISPLAY :99.0
