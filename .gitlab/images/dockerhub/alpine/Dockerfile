FROM rust:alpine AS daemon-builder

WORKDIR /usr/src/coolercontrol

RUN apk add --update nodejs npm
RUN apk add --update libc-dev openssl-dev build-base musl-dev pkgconfig perl

COPY ../../../../coolercontrol-ui ./coolercontrol-ui/
COPY ../../../../coolercontrold ./coolercontrold/
COPY ../../../../Makefile .

RUN make build-daemon


FROM alpine AS liqctld-builder

WORKDIR /usr/src/coolercontrol-liqctld

RUN apk add --update build-base python3-dev py3-pip gcc libusb musl-dev linux-headers

COPY ../../../../coolercontrol-liqctld ./

RUN python3 -m pip config set global.break-system-packages true && \
    pip install --upgrade --user .

# python:alpine images do not fully use the system-level alpine python package
#  and there are several patches involved with python for system library compatibility.
# Related issues:
#  https://github.com/python/cpython/pull/18380
#  https://github.com/docker-library/python/issues/111
FROM alpine

ENV PATH=/root/.local/bin:$PATH
ENV PATH=/usr/local/bin:$PATH
ENV CC_PORT=11987
ENV CC_HOST_IP4=0.0.0.0
ENV CC_HOST_IP6=::
ENV CC_DBUS=OFF

RUN apk add --update python3 libusb

COPY --from=daemon-builder /usr/src/coolercontrol/coolercontrold/target/release/coolercontrold /usr/local/bin/coolercontrold
COPY --from=liqctld-builder /root/.local /root/.local
COPY .gitlab/images/dockerhub/resources/entrypoint.sh /

RUN /entrypoint.sh --version

EXPOSE 11987

ENTRYPOINT ["/entrypoint.sh"]

