FROM rust AS builder

ENV DEBIAN_FRONTEND=noninteractive
# Use C.UTF-8 locale to avoid issues with ASCII encoding
ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8
ENV CI=true

ENV NODE_MAJOR=18

WORKDIR /usr/src/coolercontrol

RUN apt update && apt install -y --no-install-recommends \
    # ui
    #nodejs \
    #npm \
    # coolercontrold
    build-essential \
    libdrm-dev \
    # nodesource:
    curl \
    ca-certificates \
    gnupg

RUN mkdir -p /etc/apt/keyrings && \
    curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg && \
    echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_$NODE_MAJOR.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list && \
    apt-get update && apt-get install -y --no-install-recommends nodejs

COPY ../../../../coolercontrol-ui ./coolercontrol-ui/
COPY ../../../../coolercontrold ./coolercontrold/
COPY ../../../../Makefile .

RUN make build-daemon


FROM debian:stable-slim

ENV CC_PORT=11987
ENV CC_HOST_IP4=0.0.0.0
ENV CC_HOST_IP6=::
ENV CC_DBUS=OFF

RUN apt update && apt install -y --no-install-recommends \
    # liqctld
    gcc \
    python3-setuptools \
    make \
    python3-dev \
    python3-pip \
    libusb-1.0-0

COPY --from=builder /usr/src/coolercontrol/coolercontrold/target/release/coolercontrold /usr/local/bin/coolercontrold
COPY ../../../../coolercontrol-liqctld /usr/src/coolercontrol-liqctld/
COPY .gitlab/images/dockerhub/resources/entrypoint.sh /

RUN python3 -m pip config set global.break-system-packages true && \
    pip3 install --upgrade pip && \
    # install latest version:
    pip3 install --upgrade liquidctl && \
    make -C /usr/src/coolercontrol-liqctld install-source
RUN apt remove --purge -y make gcc python3-setuptools python3-dev python3-pip && \
    apt autoremove -y && \
    apt-get -y autoclean && \
    rm -rf /var/lib/apt/lists/* && \
    rm -rf /usr/src

RUN /entrypoint.sh --version

EXPOSE 11987

ENTRYPOINT ["/entrypoint.sh"]

