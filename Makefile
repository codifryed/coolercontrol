.DEFAULT_GOAL := help
docker_image_tag := v1
pr := poetry run

.PHONY: run help version debug lint test build \
	snap snap-clean snap-build snap-install snap-run \
	docker-clean docker-build docker-login docker-push docker-images docker-run \
	bump release

# STANDARD commands:
####################
run:
	@$(pr) coolero

help:
	@echo Possible make targets:
	@LC_ALL=C $(MAKE) -pRrq -f $(lastword $(MAKEFILE_LIST)) : 2>/dev/null | awk -v RS= -F: '/^# File/,/^# Finished Make data base/ {if ($$1 !~ "^[#.]") {print $$1}}' | sort | egrep -v -e '^[^[:alnum:]]' -e '^$@$$'

version:
	@$(pr) coolero -v
	@echo "Poetry project version: " $(shell poetry version -s)

debug:
	@$(pr) coolero --debug

lint:
	@$(pr) lint

test:
	@$(pr) test

build:
	@$(pr) build

# VERSION bumping:
##################
# Valid version arguments are:
# a valid semver string or a valid bump rule: patch, minor, major, prepatch, preminor, premajor, prerelease
# examples:
#  make bump v=minor
#  make bump v=1.2.3
v = "patch"
bump:
	@./scripts/version_bump.sh $(v)

# version from bump above applies to release as well:
release: bump
	@./scripts/release.sh

# SNAP commands:
################
snap: snap-build snap-install snap-run

snap-clean:
	@snapcraft clean

snap-build:
	@snapcraft

snap-install:
	@snap install coolero_*_amd64.snap --dangerous --classic

snap-run:
	@snap run coolero --debug

# CI DOCKER Image commands:
#####################
docker-build:
	@docker build -t registry.gitlab.com/codifryed/coolero/pipeline:$(docker_image_tag) .gitlab/

docker-login:
	@docker login registry.gitlab.com

docker-push:
	@docker push registry.gitlab.com/codifryed/coolero/pipeline:$(docker_image_tag)

docker-run:
	@docker run --name coolero-ci -v `pwd`:/app/coolero -i -t registry.gitlab.com/codifryed/coolero/pipeline:$(docker_image_tag) bash

docker-clean:
	@docker rm coolero-ci
	@docker rmi registry.gitlab.com/codifryed/coolero/pipeline:$(docker_image_tag)

docker-images:
	@docker images
