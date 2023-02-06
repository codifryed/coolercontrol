# CoolerControl-Liqctld Makefile
.DEFAULT_GOAL := build

build:
	@rm -rf coolercontrol-liqctld.dist
	@poetry env use python3.10
	@poetry install
	@poetry run python3 -m nuitka coolercontrol-liqctld.py

test: build
	# remove .bin extestion added in nuitka 1.4
	@mv coolercontrol-liqctld.dist/coolercontrol-liqctld.bin coolercontrol-liqctld.dist/coolercontrol-liqctld
	@coolercontrol-liqctld.dist/coolercontrol-liqctld --version

build-fast:
	@poetry install

test-fast: build-fast
	@poetry run coolercontrol-liqctld --version

install:
	# todo
