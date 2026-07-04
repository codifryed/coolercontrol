# CI docker image build/push/run targets (GitLab registry). Maintainer-only.
docker_image_tag := v3

.PHONY: docker-build-images docker-login docker-arm64 docker-push \
	docker-ci-run docker-ci-run-deb-bookworm docker-ci-run-deb-bookworm-arm64 \
	docker-ci-run-ubuntu docker-ci-run-ubuntu-arm64 docker-ci-run-appimage \
	docker-ci-run-cloudsmith-cli docker-clean

docker-build-images:
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/pipeline:$(docker_image_tag) -f .gitlab/images/pipeline/Dockerfile ./
	#@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag) -f .gitlab/images/bookworm/Dockerfile ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/ubuntu:$(docker_image_tag) -f .gitlab/images/ubuntu/Dockerfile ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/appimage:$(docker_image_tag) -f .gitlab/images/appimage/Dockerfile ./
	@docker build -t registry.gitlab.com/coolercontrol/coolercontrol/cloudsmith-cli:$(docker_image_tag) -f .gitlab/images/cloudsmith-cli/Dockerfile ./

docker-login:
	# this has now changed with 2FA to require a personal access token: docker login -u <username> -p <access_token> registry.gitlab.com
	@docker login registry.gitlab.com

docker-arm64:
	# This is a special build from arm64 where your system needs to be setup to be able to build aarch64 images
	@docker buildx build --platform linux/arm64 -t registry.gitlab.com/coolercontrol/coolercontrol/ubuntu-arm64:$(docker_image_tag) -f .gitlab/images/ubuntu-arm64/Dockerfile --push ./
	@docker buildx build --platform linux/arm64,linux/amd64 -t registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag) -f .gitlab/images/bookworm/Dockerfile --push ./

docker-push:
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/pipeline:$(docker_image_tag)
	#@docker push registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/ubuntu:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/appimage:$(docker_image_tag)
	@docker push registry.gitlab.com/coolercontrol/coolercontrol/cloudsmith-cli:$(docker_image_tag)

docker-ci-run:
	@docker run --name coolercontrol-ci --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/pipeline:$(docker_image_tag) bash

docker-ci-run-deb-bookworm:
	@docker run --name coolercontrol-ci-deb --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag) bash

# arm64 variant runs the same multi-arch deb-bookworm image; --platform selects the arm64 manifest (needs qemu emulation on amd64)
docker-ci-run-deb-bookworm-arm64:
	@docker run --name coolercontrol-ci-deb-arm64 --rm --platform linux/arm64 -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag) bash

docker-ci-run-ubuntu:
	@docker run --name coolercontrol-ci-ubuntu --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/ubuntu:$(docker_image_tag) bash

# arm64 variant runs the dedicated ubuntu-arm64 image (needs qemu emulation on amd64)
docker-ci-run-ubuntu-arm64:
	@docker run --name coolercontrol-ci-ubuntu-arm64 --rm --platform linux/arm64 -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/ubuntu-arm64:$(docker_image_tag) bash

docker-ci-run-appimage:
	@docker run --name coolercontrol-ci-appimage --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/appimage:$(docker_image_tag) bash

docker-ci-run-cloudsmith-cli:
	@docker run --name coolercontrol-ci-cloudsmith --rm -v `pwd`:/app/coolercontrol -i -t registry.gitlab.com/coolercontrol/coolercontrol/cloudsmith-cli:$(docker_image_tag) bash

# General:
docker-clean:
	@docker rm coolercontrol-ci || true
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/pipeline:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/deb-bookworm:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/ubuntu:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/appimage:$(docker_image_tag)
	@docker rmi registry.gitlab.com/coolercontrol/coolercontrol/cloudsmith-cli:$(docker_image_tag)
