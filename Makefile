.DEFAULT_GOAL := build-appimage
docker_image_tag := v2
appimage_docker_image_tag := v2
pr := poetry run

.PHONY: run help version debug lint test build prepare-appimage local-install docker-install build-clean \
	validate-metadata flatpak flatpak-export-deps \
	docker-clean docker-build-images docker-login docker-push docker-ci-run \
	docker-appimage-run build-appimage \
	bump release push-appimage

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
	@$(pr) pylint --rcfile=coolero/config/pylintrc coolero
	@$(pr) mypy --config-file coolero/config/mypy.ini coolero tests

test:
	@$(pr) pytest -c coolero/config/pytest.ini -k tests

build:
	@$(pr) python -m nuitka coolero.py

prepare-appimage: validate-metadata build
	@rm -f coolero.bin
	@rm -f Coolero*.AppImage*
	@mkdir -p coolero.dist/usr/share/applications
	@cp .appimage/coolero.desktop coolero.dist/usr/share/applications/org.coolero.Coolero.desktop
	@cp .appimage/coolero.desktop coolero.dist
	@mkdir -p coolero.dist/usr/share/icons/hicolor/scalable/apps
	@cp metadata/org.coolero.Coolero.svg coolero.dist/usr/share/icons/hicolor/scalable/apps/coolero.svg
	@mkdir -p coolero.dist/usr/share/icons/hicolor/256x256/apps
	@cp metadata/org.coolero.Coolero.png coolero.dist/usr/share/icons/hicolor/256x256/apps/coolero.png
	@cp metadata/org.coolero.Coolero.png coolero.dist/coolero.png
	@mkdir -p coolero.dist/usr/share/metainfo
	@cp metadata/org.coolero.Coolero.metainfo.xml coolero.dist/usr/share/metainfo
	@cp .appimage/AppImageUpdate-x86_64.AppImage coolero.dist/AppImageUpdate
	@mv coolero.dist/coolero coolero.dist/Coolero
	@mv coolero.dist/coolero_data coolero.dist/coolero
	@ln -s coolero.png coolero.dist/.DirIcon
	@cp .appimage/AppRun coolero.dist/AppRun

local-install:
	@.appimage/appimagetool-x86_64.AppImage -n -u "zsync|https://gitlab.com/api/v4/projects/30707566/packages/generic/appimage/latest/Coolero-x86_64.AppImage.zsync" --comp=xz --sign coolero.dist Coolero-x86_64.AppImage

docker-install:
	@/tmp/appimagetool-x86_64.AppImage --appimage-extract-and-run -n -u "zsync|https://gitlab.com/api/v4/projects/30707566/packages/generic/appimage/latest/Coolero-x86_64.AppImage.zsync" --comp=gzip --sign coolero.dist Coolero-x86_64.AppImage


build-clean:
	@rm -r coolero.build
	@rm -r coolero.dist

validate-metadata:
	@desktop-file-validate metadata/org.coolero.Coolero.desktop
	@appstream-util validate-relax metadata/org.coolero.Coolero.metainfo.xml

# Flatpak helpers:
##################
# for more info see the flatpak repo. Should be installed under the same parent folder as this repo

flatpak: validate-metadata
	@make -C ../org.coolero.Coolero

flatpak-export-deps:
	@poetry export -o ../org.coolero.Coolero/requirements.txt --without-hashes



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


# CI DOCKER Image commands:
#####################
docker-build-images:
	@docker build -t registry.gitlab.com/codifryed/coolero/pipeline:$(docker_image_tag) .gitlab/
	@docker rm coolero-appimage-builder || true
	@docker build -t coolero/appimagebuilder:$(appimage_docker_image_tag) .appimage/
	@docker create --name coolero-appimage-builder -v `pwd`:/app/coolero -v ~/.gnupg:/root/.gnupg -it coolero/appimagebuilder:$(appimage_docker_image_tag)

docker-login:
	@docker login registry.gitlab.com

docker-push:
	@docker push registry.gitlab.com/codifryed/coolero/pipeline:$(docker_image_tag)

docker-ci-run:
	@docker run --name coolero-ci --rm -v `pwd`:/app/coolero -i -t registry.gitlab.com/codifryed/coolero/pipeline:$(docker_image_tag) bash

# General:
docker-clean:
	@docker rm coolero-ci || true
	@docker rm coolero-appimage-builder || true
	@docker rmi registry.gitlab.com/codifryed/coolero/pipeline:$(docker_image_tag)
	@docker rmi coolero/appimagebuilder:$(appimage_docker_image_tag)

# AppImage Builder Docker commands:
##########################
docker-appimage-run:
	@docker run --name coolero-appimage-builder-test --rm -v `pwd`:/app/coolero -v ~/.gnupg:/root/.gnupg -i -t coolero/appimagebuilder:$(appimage_docker_image_tag) bash

build-appimage:
	@docker start coolero-appimage-builder --attach -i
	@echo "Setting correct file permissions."
	@sudo chown -R ${USER} coolero.dist
	@sudo chown ${USER} Coolero-x86_64.AppImage*

VERSION := $(shell poetry version -s)
push-appimage:
	@echo "Pushing AppImage v$(VERSION) to GitLab package registry"
	@curl --header "PRIVATE-TOKEN: $(COOLERO_TOKEN)" --upload-file Coolero-x86_64.AppImage "https://gitlab.com/api/v4/projects/30707566/packages/generic/appimage/$(VERSION)/Coolero-x86_64.AppImage"
	@curl --header "PRIVATE-TOKEN: $(COOLERO_TOKEN)" --upload-file Coolero-x86_64.AppImage.zsync "https://gitlab.com/api/v4/projects/30707566/packages/generic/appimage/$(VERSION)/Coolero-x86_64.AppImage.zsync"
	@curl --header "PRIVATE-TOKEN: $(COOLERO_TOKEN)" --upload-file Coolero-x86_64.AppImage "https://gitlab.com/api/v4/projects/30707566/packages/generic/appimage/latest/Coolero-x86_64.AppImage"
	@curl --header "PRIVATE-TOKEN: $(COOLERO_TOKEN)" --upload-file Coolero-x86_64.AppImage.zsync "https://gitlab.com/api/v4/projects/30707566/packages/generic/appimage/latest/Coolero-x86_64.AppImage.zsync"
