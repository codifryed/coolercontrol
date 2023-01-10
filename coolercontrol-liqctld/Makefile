# CoolerControl-Liqctld Makefile
.DEFAULT_GOAL := build

build:
	@rm -rf coolercontrol-liqctld.dist
	@poetry run python -m nuitka coolercontrol-liqctld.py
	@coolercontrol-liqctld.dist/coolercontrol-liqctld --version

install:
	# todo
