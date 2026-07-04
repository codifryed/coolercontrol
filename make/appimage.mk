# AppImage assembly. Maintainer-only.
appimage_daemon_dir := 'appimage-build-daemon'
appimage_daemon_name := 'CoolerControlD-x86_64.AppImage'
appimage_ui_dir := 'appimage-build-ui'
appimage_ui_name := 'CoolerControl-x86_64.AppImage'

.PHONY: build-appimages appimages appimage-daemon

build-appimages: build-daemon

appimages: appimage-daemon

#https://python-appimage.readthedocs.io/en/latest/
appimage-daemon:
	@$(RM) -f $(appimage_daemon_name)
	@$(RM) -rf $(appimage_daemon_dir)
	@git clone --depth=1 https://gitlab.com/coolercontrol/appimage-resources.git /tmp/resources
	@/tmp/resources/python3.*-manylinux2014_x86_64.AppImage --appimage-extract
	@squashfs-root/AppRun -s -m pip install --upgrade --no-warn-script-location liquidctl
	@$(RM) -f squashfs-root/AppRun
	@$(RM) -f squashfs-root/.DirIcon
	@$(RM) -f squashfs-root/python.png
	@$(RM) -f squashfs-root/python3.*.desktop
	@mv squashfs-root $(appimage_daemon_dir)
	@cp coolercontrold/target/release/coolercontrold $(appimage_daemon_dir)/usr/bin/
	@mkdir -p $(appimage_daemon_dir)/usr/share/applications
	@cp packaging/appimage/coolercontrold.desktop $(appimage_daemon_dir)/usr/share/applications/org.coolercontrol.CoolerControlD.desktop
	@ln -s usr/share/applications/org.coolercontrol.CoolerControlD.desktop $(appimage_daemon_dir)/coolercontrold.desktop
	@mkdir -p $(appimage_daemon_dir)/usr/share/icons/hicolor/scalable/apps
	@cp packaging/metadata/org.coolercontrol.CoolerControl.svg $(appimage_daemon_dir)/usr/share/icons/hicolor/scalable/apps/coolercontrold.svg
	@mkdir -p $(appimage_daemon_dir)/usr/share/icons/hicolor/256x256/apps
	@cp packaging/metadata/org.coolercontrol.CoolerControl.png $(appimage_daemon_dir)/usr/share/icons/hicolor/256x256/apps/coolercontrold.png
	@ln -s usr/share/icons/hicolor/256x256/apps/coolercontrold.png $(appimage_daemon_dir)/coolercontrold.png
	@mkdir -p $(appimage_daemon_dir)/usr/share/metainfo
	@cp packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml $(appimage_daemon_dir)/usr/share/metainfo
	@ln -s coolercontrold.png $(appimage_daemon_dir)/.DirIcon
	@ln -s usr/bin/coolercontrold $(appimage_daemon_dir)/AppRun
	@/tmp/resources/appimagetool-x86_64.AppImage -n --sign $(appimage_daemon_dir) $(appimage_daemon_name)
