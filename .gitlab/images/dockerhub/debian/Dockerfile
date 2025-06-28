FROM rust AS builder

ENV DEBIAN_FRONTEND=noninteractive
# Use C.UTF-8 locale to avoid issues with ASCII encoding
ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8
ENV CI=true

ENV NODE_MAJOR=18

WORKDIR /usr/src/coolercontrol
COPY ../../../../. .

RUN apt update && apt install -y --no-install-recommends \
    # ui
    nodejs \
    npm \
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

RUN make build-daemon

FROM debian:stable-slim

ENV CC_PORT=11987
ENV CC_HOST_IP4=0.0.0.0
ENV CC_HOST_IP6=::
ENV CC_DBUS=OFF

COPY --from=builder /usr/src/coolercontrol/coolercontrold/target/release/coolercontrold /usr/local/bin/coolercontrold

EXPOSE 11987

CMD ["/usr/local/bin/coolercontrold"]

