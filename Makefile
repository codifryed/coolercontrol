# CoolerControl Makefile
.DEFAULT_GOAL := build
docker_image_tag := v14
appimage_daemon_dir := 'appimage-build-daemon'
appimage_daemon_name := 'CoolerControlD-x86_64.AppImage'
appimage_gui_dir := 'appimage-build-gui'
appimage_gui_name := 'CoolerControl-x86_64.AppImage'


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
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/pipeline:$(docker_image_tag) -f .gitlab/Dockerfile-pipeline ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/deb-bullseye:$(docker_image_tag) -f .gitlab/Dockerfile-deb-bullseye ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag) -f .gitlab/Dockerfile-deb-bookworm ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/fedora-36:$(docker_image_tag) -f .gitlab/Dockerfile-fedora-36 ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/fedora-37:$(docker_image_tag) -f .gitlab/Dockerfile-fedora-37 ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/fedora-38:$(docker_image_tag) -f .gitlab/Dockerfile-fedora-38 ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/opensuse-tumbleweed:$(docker_image_tag) -f .gitlab/Dockerfile-opensuse-tumbleweed ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/cloudsmith-cli:$(docker_image_tag) -f .gitlab/Dockerfile-cloudsmith-cli ./

docker-login:
	@docker login registry.gitlab.com

docker-push:
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/pipeline:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/deb-bullseye:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/fedora-36:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/fedora-37:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/fedora-38:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/opensuse-tumbleweed:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/cloudsmith-cli:$(docker_image_tag)

docker-ci-run:
	@docker run --name coolercontrol-ci --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/pipeline:$(docker_image_tag) bash

docker-ci-run-deb-bullseye:
	@docker run --name coolercontrol-ci-deb --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/deb-bullseye:$(docker_image_tag) bash

docker-ci-run-deb-bookworm:
	@docker run --name coolercontrol-ci-deb --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag) bash

docker-ci-run-fedora-35:
	@docker run --name coolercontrol-ci-fedora --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/fedora-35:$(docker_image_tag) bash

docker-ci-run-opensuse-tumbleweed:
	@docker run --name coolercontrol-ci-opensuse-tumbleweed --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/opensuse-tumbleweed:$(docker_image_tag) bash

docker-ci-run-cloudsmit-cli:
	@docker run --name coolercontrol-ci-cloudsmith --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/cloudsmith-cli:$(docker_image_tag) bash

# General:
docker-clean:
	@docker rm coolercontrol-ci || true
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/pipeline:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/deb-bullseye:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/fedora-36:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/fedora-37:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/fedora-38:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/opensuse-tumbleweed:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/cloudsmith-cli:$(docker_image_tag)


validate-metadata:
	@desktop-file-validate packaging/metadata/org.coolercontrol.CoolerControl.desktop
	@desktop-file-validate packaging/appimage/coolercontrol.desktop
	@desktop-file-validate packaging/appimage/coolercontrold.desktop
	@appstream-util validate-relax packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml

appimage-daemon:
	@cp -f packaging/appimage/appimagetool-x86_64.AppImage /tmp/
	@sed 's|AI\x02|\x00\x00\x00|g' -i /tmp/appimagetool-x86_64.AppImage
	@rm -f $(appimage_daemon_name)
	@rm -rf $(appimage_daemon_dir)
	@mkdir $(appimage_daemon_dir)
	@cp packaging/appimage/AppImageUpdate-x86_64.AppImage $(appimage_daemon_dir)/AppImageUpdate
	@cp -rf coolercontrol-liqctld/coolercontrol-liqctld.dist/. $(appimage_daemon_dir)
	@cp coolercontrold/coolercontrold $(appimage_daemon_dir)
	@mkdir -p $(appimage_daemon_dir)/usr/share/applications
	@cp packaging/appimage/coolercontrold.desktop $(appimage_daemon_dir)/usr/share/applications/org.coolercontrol.CoolerControlD.desktop
	@cp packaging/appimage/coolercontrold.desktop $(appimage_daemon_dir)
	@mkdir -p $(appimage_daemon_dir)/usr/share/icons/hicolor/scalable/apps
	@cp packaging/metadata/org.coolercontrol.CoolerControl.svg $(appimage_daemon_dir)/usr/share/icons/hicolor/scalable/apps/coolercontrold.svg
	@mkdir -p $(appimage_daemon_dir)/usr/share/icons/hicolor/256x256/apps
	@cp packaging/metadata/org.coolercontrol.CoolerControl.png $(appimage_daemon_dir)/usr/share/icons/hicolor/256x256/apps/coolercontrold.png
	@cp packaging/metadata/org.coolercontrol.CoolerControl.png $(appimage_daemon_dir)/coolercontrold.png
	@mkdir -p $(appimage_daemon_dir)/usr/share/metainfo
	@cp packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml $(appimage_daemon_dir)/usr/share/metainfo
	@ln -s $(appimage_daemon_dir)/coolercontrold.png $(appimage_daemon_dir)/.DirIcon
	@cp packaging/appimage/AppRun-daemon $(appimage_daemon_dir)/AppRun
	@/tmp/appimagetool-x86_64.AppImage --appimage-extract-and-run -n -u "zsync|https://gitlab.com/coolercontrol/coolercontrol/-/releases/permalink/latest/downloads/packages/$(appimage_daemon_name).zsync" --comp=gzip --sign $(appimage_daemon_dir) $(appimage_daemon_name)

appimage-gui:
	@cp -f packaging/appimage/appimagetool-x86_64.AppImage /tmp/
	@sed 's|AI\x02|\x00\x00\x00|g' -i /tmp/appimagetool-x86_64.AppImage
	@rm -f $(appimage_gui_name)
	@rm -rf $(appimage_gui_dir)
	@mkdir $(appimage_gui_dir)
	@cp packaging/appimage/AppImageUpdate-x86_64.AppImage $(appimage_gui_dir)/AppImageUpdate
	@cp -rf coolercontrol-gui/coolercontrol.dist/. $(appimage_gui_dir)
	@mkdir -p $(appimage_gui_dir)/usr/share/applications
	@cp packaging/appimage/coolercontrol.desktop $(appimage_gui_dir)/usr/share/applications/org.coolercontrol.CoolerControl.desktop
	@cp packaging/appimage/coolercontrol.desktop $(appimage_gui_dir)
	@mkdir -p $(appimage_gui_dir)/usr/share/icons/hicolor/scalable/apps
	@cp packaging/metadata/org.coolercontrol.CoolerControl.svg $(appimage_gui_dir)/usr/share/icons/hicolor/scalable/apps/coolercontrol.svg
	@mkdir -p $(appimage_gui_dir)/usr/share/icons/hicolor/256x256/apps
	@cp packaging/metadata/org.coolercontrol.CoolerControl.png $(appimage_gui_dir)/usr/share/icons/hicolor/256x256/apps/coolercontrol.png
	@cp packaging/metadata/org.coolercontrol.CoolerControl.png $(appimage_gui_dir)/coolercontrol.png
	@mkdir -p $(appimage_gui_dir)/usr/share/metainfo
	@cp packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml $(appimage_gui_dir)/usr/share/metainfo
	@ln -s $(appimage_gui_dir)/coolercontrol.png $(appimage_gui_dir)/.DirIcon
	@cp packaging/appimage/AppRun-gui $(appimage_gui_dir)/AppRun
	@/tmp/appimagetool-x86_64.AppImage --appimage-extract-and-run -n -u "zsync|https://gitlab.com/coolercontrol/coolercontrol/-/releases/permalink/latest/downloads/packages/$(appimage_gui_name).zsync"  --comp=gzip --sign $(appimage_gui_dir) $(appimage_gui_name)


# VERSION bumping:
##################
# Valid version arguments are:
# a valid bump rule: patch, minor, major
# examples:
#  make bump v=minor
v = "patch"
bump:
	@./packaging/version_bump.sh $(v)

# version from bump above applies to release as well:
release: bump
	@./packaging/release.sh

push-release:
	@git push --follow-tags
