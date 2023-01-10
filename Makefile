# CoolerControl Makefile
.DEFAULT_GOAL := build

# can be run in parallel with make -j3
build: build-liqctld build-daemon build-gui

build-liqctld:
	@$(MAKE) -C coolercontrol-liqctld

build-daemon:
	@$(MAKE) -C coolercontrold

build-gui:
	@$(MAKE) -C coolercontrol-gui
