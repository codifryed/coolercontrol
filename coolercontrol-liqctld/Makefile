# CoolerControl-Liqctld Makefile
.DEFAULT_GOAL := build

build:
	@rm -rf coolercontrol-liqctld.dist
	@poetry install
	@poetry run python -m nuitka coolercontrol-liqctld.py

test: build
	@coolercontrol-liqctld.dist/coolercontrol-liqctld --version

build-fast:
	@poetry install

test-fast: build-fast
	@poetry run liqctld --version

install:
	# todo
