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

EXPOSE 11987

CMD ["/usr/local/bin/coolercontrold"]

