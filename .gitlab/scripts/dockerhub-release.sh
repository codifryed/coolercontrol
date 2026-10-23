#!/bin/sh
# SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Usage: dockerhub-release.sh debian|alpine
#
# A rehearsal stages the image under a commit-keyed tag; a tag gives the rehearsal's exact digest
# its release tags (RELEASING.md). POSIX sh for the docker:dind image.

set -eu

readonly IMAGE="${DH_REGISTRY_IMAGE:?}"
case "${1-}" in
debian)
    readonly PREFIX=""
    readonly RELEASE_TAGS="${CI_COMMIT_TAG-} latest"
    ;;
alpine)
    readonly PREFIX="alpine-"
    readonly RELEASE_TAGS="alpine-${CI_COMMIT_TAG-} alpine-latest alpine"
    ;;
*)
    echo "usage: $0 debian|alpine" >&2
    exit 2
    ;;
esac
readonly VARIANT="$1"
readonly STAGING_TAG="${PREFIX}prerelease-${CI_COMMIT_SHA:?}"
readonly DIGEST_FILE="dockerhub-${VARIANT}.digest"

echo "${DH_REGISTRY_PASSWORD:?}" | docker login -u "${DH_REGISTRY_USER:?}" --password-stdin "${DH_URI:?}"
apk add curl jq
arch="${CI_RUNNER_EXECUTABLE_ARCH#*/}"
buildx_url=$(curl -s https://raw.githubusercontent.com/docker/actions-toolkit/main/.github/buildx-lab-releases.json |
    jq -r ".latest.assets[] | select(endswith(\"linux-${arch}\"))")
mkdir -p ~/.docker/cli-plugins/
curl --silent --location --output ~/.docker/cli-plugins/docker-buildx "${buildx_url}"
chmod a+x ~/.docker/cli-plugins/docker-buildx

if [ -z "${CI_COMMIT_TAG-}" ]; then
    docker buildx create --use --driver cloud "${DH_ORG:?}/${DH_CLOUD_BUILDER_NAME:?}"
    docker buildx build --platform linux/amd64,linux/arm64 --tag "${IMAGE}:${STAGING_TAG}" \
        -f ".gitlab/images/dockerhub/${VARIANT}/Dockerfile" --metadata-file /tmp/metadata.json \
        --push .
    jq -er '."containerimage.digest"' /tmp/metadata.json >"${DIGEST_FILE}"
    digest=$(cat "${DIGEST_FILE}")
    echo "Staged the ${VARIANT} image as ${IMAGE}:${STAGING_TAG} (${digest})"
    exit 0
fi

apk add python3
python3 .gitlab/scripts/release_rehearsal.py fetch "${CI_JOB_NAME:?}"
digest=$(cat "${DIGEST_FILE}")
case "${digest}" in
sha256:*) ;;
*)
    echo "${DIGEST_FILE} holds no image digest: ${digest}" >&2
    exit 1
    ;;
esac

tag_args=""
for tag in ${RELEASE_TAGS}; do
    tag_args="${tag_args} --tag ${IMAGE}:${tag}"
done
# shellcheck disable=SC2086 # one --tag word pair per release tag
docker buildx imagetools create ${tag_args} "${IMAGE}@${digest}"
echo "Published ${IMAGE}@${digest} as ${RELEASE_TAGS}"

# The release tags keep the image; only the staging name goes. A failure here leaves a stale tag,
# not a broken release.
repository="${IMAGE#*/}"
# jq reads the credentials from the environment, so they stay out of every argv.
token=$(jq -n '{username: env.DH_REGISTRY_USER, password: env.DH_REGISTRY_PASSWORD}' |
    curl --silent --header 'Content-Type: application/json' --data @- \
        https://hub.docker.com/v2/users/login/ | jq -r '.token // empty')
# The header goes in on stdin (printf is a builtin), keeping the token out of curl's argv.
if [ -n "${token}" ] && printf 'Authorization: JWT %s\n' "${token}" |
    curl --fail --silent --request DELETE --header @- \
        "https://hub.docker.com/v2/repositories/${repository}/tags/${STAGING_TAG}/"; then
    echo "Deleted the staging tag ${STAGING_TAG}"
else
    echo "Could not delete the staging tag ${STAGING_TAG}; remove it on Docker Hub."
fi
