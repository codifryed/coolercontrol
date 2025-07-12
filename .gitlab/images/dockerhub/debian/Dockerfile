FROM rust AS daemon-builder

ENV DEBIAN_FRONTEND=noninteractive
ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8
ENV CI=true

ENV NODE_MAJOR=18

WORKDIR /usr/src/coolercontrol

RUN apt update && apt install -y --no-install-recommends \
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


FROM python AS liqctld-builder

ENV DEBIAN_FRONTEND=noninteractive
ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8
ENV CI=true

WORKDIR /usr/src/coolercontrol-liqctld

RUN apt update && apt install -y --no-install-recommends \
    build-essential \
    gcc \
    python3-dev \
    libusb-1.0-0

COPY ../../../../coolercontrol-liqctld ./

RUN pip install --upgrade --user -r requirements.txt

RUN pip install --user .


FROM python:slim

ENV PATH=/root/.local/bin:$PATH
ENV PATH=/usr/local/bin:$PATH
ENV CC_PORT=11987
ENV CC_HOST_IP4=0.0.0.0
ENV CC_HOST_IP6=::
ENV CC_DBUS=OFF

RUN apt update && apt install -y --no-install-recommends \
    libusb-1.0-0 && \
    rm -rf /var/cache/apt/archives /var/lib/apt/lists/*

COPY --from=daemon-builder /usr/src/coolercontrol/coolercontrold/target/release/coolercontrold /usr/local/bin/coolercontrold
COPY --from=liqctld-builder /root/.local /root/.local
COPY .gitlab/images/dockerhub/resources/entrypoint.sh /

RUN /entrypoint.sh --version

EXPOSE 11987

ENTRYPOINT ["/entrypoint.sh"]

