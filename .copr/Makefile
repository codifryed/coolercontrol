#COPR build target
daemon := coolercontrold
liqctld := coolercontrol-liqctld
ui := coolercontrol

srpm: install-rpkg
ifneq (,$(findstring $(liqctld).spec, $(spec)))
	cp ../packaging/fedora/$(liqctld).spec ./
	cp ../packaging/systemd/$(liqctld).service ./
	rpkg srpm --outdir=$(outdir) --spec=$(spec)
else ifneq (,$(findstring $(daemon).spec, $(spec)))
	cp ../packaging/fedora/$(daemon).spec ./
	cp ../packaging/systemd/$(daemon).service ./
	cp -rf ../$(ui)-ui ./
	rpkg srpm --outdir=$(outdir) --spec=$(spec)
else ifneq (,$(findstring $(ui).spec, $(spec)))
	cp ../packaging/fedora/$(ui).spec ./
	cp -rf ../packaging/metadata ./
	rpkg srpm --outdir=$(outdir) --spec=$(spec)
else
	@echo "No package spec found for $(spec)"
endif

install-rpkg:
	dnf -y install rpkg
