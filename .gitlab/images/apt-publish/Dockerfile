FROM debian:bookworm-slim

LABEL maintainer=codifryed
# Image that publishes debs into the apt repository on Cloudflare R2

ENV DEBIAN_FRONTEND=noninteractive
# Use C.UTF-8 locale to avoid issues with ASCII encoding
ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8
ENV CI=true

RUN apt-get update && apt-get install -y --no-install-recommends \
    aptly \
    rclone \
    dpkg \
    gnupg \
    ca-certificates \
    curl && \
    apt-get -y autoclean && \
    rm -rf /var/lib/apt/lists/*

# The keyring and installer are baked in so a publish job only needs the deb files and the
# credentials.
COPY packaging/apt-repo/apt-repo-publish.sh /usr/local/bin/apt-repo-publish
COPY packaging/apt-repo/keys /usr/local/share/apt-repo/keys
COPY packaging/apt-repo/setup.sh /usr/local/share/apt-repo/setup.sh

RUN chmod 755 /usr/local/bin/apt-repo-publish
