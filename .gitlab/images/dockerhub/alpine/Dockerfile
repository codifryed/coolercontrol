FROM rust:alpine AS daemon-builder

WORKDIR /usr/src/coolercontrol

RUN apk add --update nodejs npm
RUN apk add --update libc-dev openssl-dev build-base musl-dev pkgconfig perl

COPY ../../../../coolercontrol-ui ./coolercontrol-ui/
COPY ../../../../coolercontrold ./coolercontrold/
COPY ../../../../Makefile .

RUN make build-daemon


FROM python:alpine AS liqctld-builder

WORKDIR /usr/src/coolercontrol-liqctld

RUN apk add --update build-base gcc python3-dev py3-pip libusb musl-dev linux-headers

COPY ../../../../coolercontrol-liqctld ./

RUN pip install --upgrade --user -r requirements.txt

RUN pip install --user .


FROM python:alpine

ENV PATH=/root/.local/bin:$PATH
ENV PATH=/usr/local/bin:$PATH
ENV CC_PORT=11987
ENV CC_HOST_IP4=0.0.0.0
ENV CC_HOST_IP6=::
ENV CC_DBUS=OFF

COPY --from=daemon-builder /usr/src/coolercontrol/coolercontrold/target/release/coolercontrold /usr/local/bin/coolercontrold
COPY --from=liqctld-builder /root/.local /root/.local
COPY .gitlab/images/dockerhub/resources/entrypoint.sh /


RUN /entrypoint.sh --version

EXPOSE 11987

ENTRYPOINT ["/entrypoint.sh"]

