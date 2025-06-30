FROM rust:alpine AS builder

WORKDIR /usr/src/coolercontrol
COPY ../../../../. .

RUN apk add --update nodejs npm
RUN apk add --update libc-dev openssl-dev build-base musl-dev pkgconfig perl

RUN make build-daemon

FROM alpine

ENV CC_PORT=11987
ENV CC_HOST_IP4=0.0.0.0
ENV CC_HOST_IP6=::
ENV CC_DBUS=OFF

COPY --from=builder /usr/src/coolercontrol/coolercontrold/target/release/coolercontrold /usr/local/bin/coolercontrold
COPY ../../../../coolercontrol-liqctld/. /usr/src/coolercontrol-liqctld
COPY entrypoint.sh /

RUN apk add --update make gcc liquidctl python3-dev py3-pip libusb

RUN python3 -m pip config set global.break-system-packages true && \
    pip3 install --upgrade pip && \
    make -C /usr/src/coolercontrol-liqctld install-source
RUN apk del make gcc python3-dev py3-pip && \
    rm -rf /usr/src

EXPOSE 11987

ENTRYPOINT ["/entrypoint.sh"]

