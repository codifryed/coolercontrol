%global _enable_debug_packages 0
# stripping messes with nuitka-build linking
%undefine __brp_strip
%undefine __brp_strip_static_archive

Name:           coolercontrol
Version:        0.14.1
Release:        0%{?dist}
Summary:        Monitor and control your cooling devices.

License:        GPLv3+
URL:            https://gitlab.com/coolercontrol/coolercontrol

BuildRequires:  systemd-rpm-macros libappstream-glib
Requires:       hicolor-icon-theme
BuildArch:      x86_64
Source0:        CoolerControl.tar.gz
# find-requires and find-provides doesn't work as intended due to nuitka-build linking
AutoReqProv: no

%description
CoolerControl is a program to monitor and control your cooling devices.

It offers an easy-to-use user interface with various control features and also provides live thermal performance details.

%prep
cp %{_sourcedir}/CoolerControl/* %{_builddir} -r

%build

%install
mkdir -p %{buildroot}%{_bindir}
install -p -m 755 coolercontrold/coolercontrold %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_datadir}/%{name}/liqctld/
cp -pr coolercontrol-liqctld/coolercontrol-liqctld.dist/. %{buildroot}%{_datadir}/%{name}/liqctld/
ln -s ../../..%{_datadir}/%{name}/liqctld/coolercontrol-liqctld %{buildroot}%{_bindir}/coolercontrol-liqctld
mkdir -p %{buildroot}%{_datadir}/%{name}/gui/
cp -pr coolercontrol-gui/coolercontrol.dist/. %{buildroot}%{_datadir}/%{name}/gui/
ln -s ../../..%{_datadir}/%{name}/gui/coolercontrol-gui %{buildroot}%{_bindir}/coolercontrol
#desktop
desktop-file-install packaging/metadata/org.coolercontrol.CoolerControl.desktop
mkdir -p %{buildroot}%{_datadir}/icons/hicolor/scalable/apps
cp -pr packaging/metadata/org.coolercontrol.CoolerControl.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps
mkdir -p %{buildroot}%{_datadir}/icons/hicolor/256x256/apps
cp -pr packaging/metadata/org.coolercontrol.CoolerControl.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps
mkdir -p %{buildroot}%{_metainfodir}
cp -pr packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml %{buildroot}%{_metainfodir}
mkdir -p %{buildroot}%{_unitdir}
cp -p packaging/systemd/coolercontrol-liqctld.service %{buildroot}%{_unitdir}
cp -p packaging/systemd/coolercontrold.service %{buildroot}%{_unitdir}

%check
appstream-util validate-relax --nonet %{buildroot}%{_metainfodir}/*.metainfo.xml
%{buildroot}%{_bindir}/coolercontrold --version
%{buildroot}%{_datadir}/%{name}/gui/coolercontrol-gui --version
%{buildroot}%{_datadir}/%{name}/liqctld/coolercontrol-liqctld --version

%files
%{_bindir}/%{name}
%{_bindir}/coolercontrold
%{_bindir}/coolercontrol-liqctld
%{_datadir}/applications/org.%{name}.CoolerControl.desktop
%{_datadir}/icons/hicolor/scalable/apps/org.%{name}.CoolerControl.svg
%{_datadir}/icons/hicolor/256x256/apps/org.%{name}.CoolerControl.png
%{_metainfodir}/org.%{name}.CoolerControl.metainfo.xml
%{_unitdir}/coolercontrol-liqctld.service
%{_unitdir}/coolercontrold.service
%{_datadir}/%{name}/liqctld/
%{_datadir}/%{name}/gui/
%license LICENSE
%doc README.md CHANGELOG.md

%changelog
* Sun Feb 06 2023 Guy Boldon <gb@guyboldon.com> - 0.14.1-0
- 0.14.1 Release

* Sun Jan 15 2023 Guy Boldon <gb@guyboldon.com> - 0.14.0-0
- 0.14.0 Release
