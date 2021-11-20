.DEFAULT_GOAL := help
docker_image_tag := v1
pr := poetry run

.PHONY: run
# STANDARD commands:
####################
run:
	@$(pr) coolero

.PHONY: help
help:
	@echo Possible make targets:
	@LC_ALL=C $(MAKE) -pRrq -f $(lastword $(MAKEFILE_LIST)) : 2>/dev/null | awk -v RS= -F: '/^# File/,/^# Finished Make data base/ {if ($$1 !~ "^[#.]") {print $$1}}' | sort | egrep -v -e '^[^[:alnum:]]' -e '^$@$$'

.PHONY: version
version:
	@$(pr) coolero -v
	@echo "Poetry project version: " $(shell poetry version -s)

.PHONY: debug
debug:
	@$(pr) coolero --debug

.PHONY: lint
lint:
	@$(pr) lint

.PHONY: test
test:
	@$(pr) test

.PHONY: build
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

docker-build:
	@docker build -t registry.gitlab.com/codifryed/coolero/pipeline:$(docker_image_tag) .gitlab/

.PHONY: docker-login
docker-login:
	@docker login registry.gitlab.com

.PHONY: docker-push
docker-push:
	@docker push registry.gitlab.com/codifryed/coolero/pipeline:$(docker_image_tag)

.PHONY: docker-run
docker-run:
	@docker run --name coolero-ci -v `pwd`:/app/coolero -i -t registry.gitlab.com/codifryed/coolero/pipeline:$(docker_image_tag) bash

.PHONY: docker-clean
docker-clean:
	@docker rm coolero-ci
	@docker rmi registry.gitlab.com/codifryed/coolero/pipeline:$(docker_image_tag)

.PHONY: docker-images
docker-images:
	@docker images
