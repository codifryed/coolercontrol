# CoolerControl Makefile
.DEFAULT_GOAL := build
docker_image_tag := v18
ap_id := 'org.coolercontrol.CoolerControl'
liqctld_dir := 'coolercontrol-liqctld'
daemon_dir := 'coolercontrold'
ui_dir := 'coolercontrol-ui'
tauri_dir := '$(ui_dir)/src-tauri'
appimage_daemon_dir := 'appimage-build-daemon'
appimage_daemon_name := 'CoolerControlD-x86_64.AppImage'
appimage_ui_dir := 'appimage-build-ui'
appimage_ui_name := 'CoolerControl-x86_64.AppImage'

.PHONY: build build-ui build-source build-appimages build-liqctld-binary test clean install install-source uninstall \
		appimages bump release push-release validate-metadata

# Release goals
# can be run in parallel with make -j2
build: build-daemon build-tauri

build-daemon: build-ui
	@$(MAKE) -C $(daemon_dir) build

build-tauri: build-ui
	@$(MAKE) -C $(tauri_dir) build

build-ui:
	@$(MAKE) -C $(ui_dir) build

build-source: build
	@$(MAKE) -C $(liqctld_dir) $@

# parallelize with make -j3
build-appimages: build-daemon build-tauri-appimage build-liqctld-binary

build-liqctld-binary:
	@$(MAKE) -C $(liqctld_dir) build-binary

build-tauri-appimage:
	@$(MAKE) -C $(tauri_dir) build-appimage

build-offline: build-daemon-offline build-tauri-offline

build-daemon-offline: build-ui-offline
	@$(MAKE) -C $(daemon_dir) build

build-tauri-offline: build-ui-offline
	@$(MAKE) -C $(tauri_dir) build

build-ui-offline:
	@$(MAKE) -C $(ui_dir) offline

# parallelize with make -j4
test: validate-metadata test-liqctld test-daemon test-ui test-tauri

test-liqctld:
	@$(MAKE) -C $(liqctld_dir) test

test-daemon:
	@$(MAKE) -C $(daemon_dir) test

test-ui:
	@$(MAKE) -C $(ui_dir) test

test-tauri: test-ui
	@$(MAKE) -C $(tauri_dir) test
 
ci-test: validate-metadata ci-test-liqctld ci-test-daemon ci-test-ui ci-test-tauri

ci-test-liqctld:
	@$(MAKE) -C $(liqctld_dir) ci-test

ci-test-daemon:
	@$(MAKE) -C $(daemon_dir) ci-test

ci-test-ui:
	@$(MAKE) -C $(ui_dir) ci-test

ci-test-tauri: ci-test-ui
	@$(MAKE) -C $(tauri_dir) ci-test

ci-check-all:
	@./trunk check --ci --all

ci-check:
	@./trunk check --ci

ci-fmt:
	@./trunk fmt --all

clean:
	@$(MAKE) -C $(liqctld_dir) $@
	@$(MAKE) -C $(daemon_dir) $@
	@$(MAKE) -C $(ui_dir) $@
	@$(MAKE) -C $(tauri_dir) $@

install:
	@$(MAKE) -C $(daemon_dir) $@
	@$(MAKE) -C $(tauri_dir) $@

install-source: build-source install
	@$(MAKE) -C $(liqctld_dir) $@
	@install -Dm644 packaging/metadata/$(ap_id).desktop -t $(DESTDIR)/usr/local/share/applications/
	@install -Dm644 packaging/metadata/$(ap_id).metainfo.xml -t $(DESTDIR)/usr/share/metainfo/
	@install -Dm644 packaging/metadata/$(ap_id).png -t $(DESTDIR)/usr/share/pixmaps/
	@install -Dm644 packaging/metadata/$(ap_id).svg -t $(DESTDIR)/usr/share/icons/hicolor/scalable/apps/
	@install -Dm644 packaging/systemd/coolercontrold.service -t $(DESTDIR)/etc/systemd/system/
	@install -Dm644 packaging/systemd/coolercontrol-liqctld.service -t $(DESTDIR)/etc/systemd/system/

uninstall:
	@$(MAKE) -C $(liqctld_dir) $@
	@$(MAKE) -C $(daemon_dir) $@
	@$(MAKE) -C $(tauri_dir) $@
	@-rm -f $(DESTDIR)/usr/local/share/applications/$(ap_id).desktop
	@-rm -f $(DESTDIR)/usr/share/metainfo/$(ap_id).metainfo.xml
	@-rm -f $(DESTDIR)/usr/share/pixmaps/$(ap_id).png
	@-rm -f $(DESTDIR)/usr/share/icons/hicolor/scalable/apps/$(ap_id).svg
	@-rm -f $(DESTDIR)/etc/systemd/system/coolercontrold.service
	@-rm -f $(DESTDIR)/etc/systemd/system/coolercontrol-liqctld.service

dev-build: clean build

dev-install:
	@sudo $(MAKE) install
	@sudo systemctl restart coolercontrold

validate-metadata:
	@appstream-util --version || true
	@desktop-file-validate packaging/metadata/org.coolercontrol.CoolerControl.desktop
	@desktop-file-validate packaging/appimage/coolercontrol.desktop
	@desktop-file-validate packaging/appimage/coolercontrold.desktop
	@appstream-util validate-relax packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml

ubuntu-source-package:
	@sed -i 's/UNRELEASED/jammy/g' debian/changelog
	@debuild -S -sa --force-sign
	@cd .. && dput ppa:codifryed/coolercontrol ../coolercontrol_*_source.changes


# AppImages:
############################################################################################################################################

appimages: appimage-daemon appimage-ui

appimage-daemon:
	@cp -f packaging/appimage/appimagetool-x86_64.AppImage /tmp/
	@sed 's|AI\x02|\x00\x00\x00|g' -i /tmp/appimagetool-x86_64.AppImage
	@rm -f $(appimage_daemon_name)
	@rm -rf $(appimage_daemon_dir)
	@mkdir $(appimage_daemon_dir)
	@cp -rf coolercontrol-liqctld/liqctld.dist/. $(appimage_daemon_dir)
	@cp coolercontrold/target/release/coolercontrold $(appimage_daemon_dir)
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
	@/tmp/appimagetool-x86_64.AppImage -n --comp=gzip --sign $(appimage_daemon_dir) $(appimage_daemon_name)

appimage-ui:
	@cp coolercontrol-ui/src-tauri/target/release/bundle/appimage/coolercontrol_*_amd64.AppImage $(appimage_ui_name)


# Release
############################################################################################################################################
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


# CI DOCKER Image commands:
############################################################################################################################################
docker-build-images:
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/pipeline:$(docker_image_tag) -f .gitlab/images/pipeline/Dockerfile ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/deb-bullseye:$(docker_image_tag) -f .gitlab/images/bullseye/Dockerfile ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag) -f .gitlab/images/bookworm/Dockerfile ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/ubuntu:$(docker_image_tag) -f .gitlab/images/ubuntu/Dockerfile ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/cloudsmith-cli:$(docker_image_tag) -f .gitlab/images/cloudsmith-cli/Dockerfile ./

docker-login:
	# this has now changed with 2FA to require a personal access token: docker login -u <username> -p <access_token> registry.gitlab.com
	@docker login registry.gitlab.com

docker-push:
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/pipeline:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/deb-bullseye:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/ubuntu:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/cloudsmith-cli:$(docker_image_tag)

docker-ci-run:
	@docker run --name coolercontrol-ci --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/pipeline:$(docker_image_tag) bash

docker-ci-run-deb-bullseye:
	@docker run --name coolercontrol-ci-deb --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/deb-bullseye:$(docker_image_tag) bash

docker-ci-run-deb-bookworm:
	@docker run --name coolercontrol-ci-deb --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag) bash

docker-ci-run-ubuntu:
	@docker run --name coolercontrol-ci-ubuntu --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/ubuntu:$(docker_image_tag) bash

docker-ci-run-cloudsmith-cli:
	@docker run --name coolercontrol-ci-cloudsmith --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/cloudsmith-cli:$(docker_image_tag) bash

# General:
docker-clean:
	@docker rm coolercontrol-ci || true
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/pipeline:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/deb-bullseye:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/ubuntu:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/cloudsmith-cli:$(docker_image_tag)
