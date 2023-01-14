# CoolerControl Makefile
.DEFAULT_GOAL := build
docker_image_tag := v14


# Release goals
# can be run in parallel with make -j3
build: build-liqctld build-daemon build-gui

build-liqctld:
	@$(MAKE) -C coolercontrol-liqctld

build-daemon:
	@$(MAKE) -C coolercontrold

build-gui:
	@$(MAKE) -C coolercontrol-gui


# Release Test goals
test: test-liqctld test-daemon test-gui

test-liqctld: build-liqctld
	@$(MAKE) -C coolercontrol-liqctld

test-daemon: build-daemon
	@$(MAKE) -C coolercontrold

test-gui: build-gui
	@$(MAKE) -C coolercontrol-gui


# Fast build goals
build-fast: build-fast-liqctld build-fast-daemon build-fast-gui

build-fast-liqctld:
	@$(MAKE) -C coolercontrol-liqctld build-fast

build-fast-daemon:
	@$(MAKE) -C coolercontrold build-fast

build-fast-gui:
	@$(MAKE) -C coolercontrol-gui build-fast


# Fast test goals
test-fast: test-fast-liqctld test-fast-daemon test-fast-gui

test-fast-liqctld: build-fast-liqctld
	@$(MAKE) -C coolercontrol-liqctld build-fast

test-fast-daemon: build-fast-daemon
	@$(MAKE) -C coolercontrold build-fast

test-fast-gui: build-fast-gui
	@$(MAKE) -C coolercontrol-gui build-fast


# CI DOCKER Image commands:
#####################
docker-build-images:
	@docker build -t registry.gitlab.com/coolero/coolero/pipeline:$(docker_image_tag) .gitlab/

docker-login:
	@docker login registry.gitlab.com

docker-push:
	@docker push registry.gitlab.com/coolero/coolero/pipeline:$(docker_image_tag)

docker-ci-run:
	@docker run --name coolercontrol-ci --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolero/coolero/pipeline:$(docker_image_tag) bash

# General:
docker-clean:
	@docker rm coolercontrol-ci || true
	@docker rmi registry.gitlab.com/coolero/coolero/pipeline:$(docker_image_tag)
