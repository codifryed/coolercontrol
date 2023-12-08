#COPR build target
project := coolercontrol
daemon := $(project)d
liqctld := $(project)-liqctld
ui := $(project)-ui

#ifneq (,$(findstring $(liqctld).spec, $(spec)))
#else ifneq (,$(findstring $(daemon).spec, $(spec)))
#	rpkg srpm --outdir=$(outdir) --spec=$(spec)
#else ifneq (,$(findstring $(ui).spec, $(spec)))
#else
#	@echo "No package spec found for $(spec)"
#endif

srpm: install-rpkg copy-files
	rpkg status
	ls -lah
	rpkg  --config-file=../.copr/rpkg.conf srpm --outdir=$(outdir) --spec=$(spec)
	echo "Done"

install-rpkg:
	dnf -y install rpkg

copy-files:
	cp ../LICENSE ../$(liqctld)/
	cp ../CHANGELOG.md ../$(liqctld)/
	cp ../packaging/fedora/$(liqctld).spec ../$(liqctld)/
	cp ../packaging/systemd/$(liqctld).service ../$(liqctld)/
	cp ../packaging/fedora/$(daemon).spec ../$(daemon)/
	cp ../packaging/systemd/$(daemon).service ../$(daemon)/
	cp -rf ../$(ui) ../$(daemon)/
	cp ../packaging/fedora/$(project).spec ../$(ui)/
	cp -rf ../packaging/metadata ../$(ui)/
