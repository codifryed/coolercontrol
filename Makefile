# CoolerControl Makefile
.DEFAULT_GOAL := build
docker_image_tag := v14


# Release goals
# can be run in parallel with make -j3
build: build-liqctld build-daemon build-gui

build-liqctld:
	@$(MAKE) -C coolercontrol-liqctld build

build-daemon:
	@$(MAKE) -C coolercontrold build

build-gui:
	@$(MAKE) -C coolercontrol-gui build


# Release Test goals
test: test-liqctld test-daemon test-gui

test-liqctld:
	@$(MAKE) -C coolercontrol-liqctld test

test-daemon:
	@$(MAKE) -C coolercontrold test

test-gui:
	@$(MAKE) -C coolercontrol-gui test


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

test-fast-liqctld:
	@$(MAKE) -C coolercontrol-liqctld test-fast

test-fast-daemon:
	@$(MAKE) -C coolercontrold test-fast

test-fast-gui:
	@$(MAKE) -C coolercontrol-gui test-fast


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

appimage-daemon:
	@cp -f packaging/appimage/appimagetool-x86_64.AppImage /tmp/
	@sed 's|AI\x02|\x00\x00\x00|g' -i /tmp/appimagetool-x86_64.AppImage
	@mkdir appimage-build
	@mv coolercontrol-liqctld/coolercontrol-liqctld.dist/* appimage-build/
	@mv coolercontrold/coolercontrold appimage-build/
	@mkdir -p appimage-build/usr/share/applications
	@cp packaging/appimage/coolercontrold.desktop appimage-build/usr/share/applications/org.coolercontrol.CoolerControlD.desktop
	@cp packaging/appimage/coolercontrold.desktop appimage-build
	@mkdir -p appimage-build/usr/share/icons/hicolor/scalable/apps
	@cp packaging/metadata/org.coolercontrol.CoolerControl.svg appimage-build/usr/share/icons/hicolor/scalable/apps/coolercontrold.svg
	@mkdir -p appimage-build/usr/share/icons/hicolor/256x256/apps
	@cp packaging/metadata/org.coolercontrol.CoolerControl.png appimage-build/usr/share/icons/hicolor/256x256/apps/coolercontrold.png
	@cp packaging/metadata/org.coolercontrol.CoolerControl.png appimage-build/coolercontrold.png
	@mkdir -p appimage-build/usr/share/metainfo
	@cp packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml appimage-build/usr/share/metainfo
	@ln -s appimage-build/coolercontrold.png appimage-build/.DirIcon
	@cp packaging/appimage/AppRun-daemon appimage-build/AppRun
	@/tmp/appimagetool-x86_64.AppImage --appimage-extract-and-run -n --comp=gzip appimage-build CoolerControlD-x86_64.AppImage
