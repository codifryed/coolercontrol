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
    dpkg \
    gnupg \
    ca-certificates \
    curl \
    unzip && \
    apt-get -y autoclean && \
    rm -rf /var/lib/apt/lists/*

# Not the packaged rclone: bookworm ships 1.60.1, which re-reads an object by the versionId R2
# returns from a PUT. R2 does not implement versioned reads and answers 501, so every fresh
# upload failed and only "succeeded" on retry by being skipped.
# ARG, not ENV: rclone maps every RCLONE_* variable to a flag, so a persisted RCLONE_VERSION
# would be read as --version and abort every invocation.
ARG RCLONE_VERSION=1.75.0
RUN arch=$(dpkg --print-architecture) && \
    case "${arch}" in \
    amd64) sha=aa2804e08f48250e71009c727124b6341cd0288465804a9a09d14663cabafbaa ;; \
    arm64) sha=d0ad88ba4c8e285b7c9efa591e0ab643280a91741e13c27f3a9c0957ccfa5203 ;; \
    *) echo "no pinned rclone checksum for ${arch}" >&2 && exit 1 ;; \
    esac && \
    zip=rclone-v${RCLONE_VERSION}-linux-${arch}.zip && \
    curl -fsSLO "https://downloads.rclone.org/v${RCLONE_VERSION}/${zip}" && \
    echo "${sha}  ${zip}" | sha256sum -c - && \
    unzip -qj "${zip}" "rclone-v${RCLONE_VERSION}-linux-${arch}/rclone" -d /usr/local/bin && \
    chmod 755 /usr/local/bin/rclone && \
    rm -f "${zip}" && \
    rclone version | head -1

# The keyring and installer are baked in so a publish job only needs the deb files and the
# credentials.
COPY packaging/apt-repo/apt-repo-publish.sh /usr/local/bin/apt-repo-publish
COPY packaging/apt-repo/keys /usr/local/share/apt-repo/keys
COPY packaging/apt-repo/setup.sh /usr/local/share/apt-repo/setup.sh

RUN chmod 755 /usr/local/bin/apt-repo-publish
