.DEFAULT_GOAL := help
image_tag = v1
pr = poetry run

.PHONY: run
run:
	@$(pr) coolero

.PHONY: help
help:
	@echo Possible make targets:
	@LC_ALL=C $(MAKE) -pRrq -f $(lastword $(MAKEFILE_LIST)) : 2>/dev/null | awk -v RS= -F: '/^# File/,/^# Finished Make data base/ {if ($$1 !~ "^[#.]") {print $$1}}' | sort | egrep -v -e '^[^[:alnum:]]' -e '^$@$$'

.PHONY: version
version:
	@$(pr) coolero -v

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

.PHONY: docker-build
docker-build:
	@docker build -t registry.gitlab.com/codifryed/coolero/pipeline:$(image_tag) .gitlab/

.PHONY: docker-login
docker-login:
	@docker login registry.gitlab.com

.PHONY: docker-push
docker-push:
	@docker push registry.gitlab.com/codifryed/coolero/pipeline:$(image_tag)

.PHONY: docker-run
docker-run:
	@docker run --name coolero-ci -v `pwd`:/app/coolero -i -t registry.gitlab.com/codifryed/coolero/pipeline:$(image_tag) bash

.PHONY: docker-clean
docker-clean:
	@docker rm coolero-ci
	@docker rmi registry.gitlab.com/codifryed/coolero/pipeline:$(image_tag)

.PHONY: docker-images
docker-images:
	@docker images