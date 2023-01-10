# CoolerControl Makefile
.DEFAULT_GOAL := build

build:
	@poetry run python -m nuitka coolercontrol.py
	@mv coolercontrol.dist/coolercontrol coolercontrol.dist/coolercontrol-gui
	@mv coolercontrol.dist/coolercontrol_data coolercontrol.dist/coolercontrol
	@coolercontrol.dist/coolercontrol-gui --version

install:
	# todo
