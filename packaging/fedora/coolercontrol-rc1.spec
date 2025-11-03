%global branch main
%global project_id 30707566
%global qt_dir coolercontrol
%global ap_id org.coolercontrol.CoolerControl

Name:           coolercontrol
Version:        3.0.3~rc1
Release:        %autorelease
Summary:        Powerful cooling control and monitoring
ExclusiveArch:  x86_64 aarch64
License:        GPL-3.0-or-later
URL:            https://gitlab.com/%{name}/%{name}

BuildRequires:  libappstream-glib
BuildRequires:  desktop-file-utils
BuildRequires:  make
BuildRequires:  cmake
BuildRequires:  autoconf automake gcc gcc-c++
BuildRequires:  cmake(Qt6)
BuildRequires:  cmake(Qt6WebEngineCore)
BuildRequires:  cmake(Qt6WebEngineWidgets)
BuildRequires:  cmake(Qt6WebChannel)
Requires:       hicolor-icon-theme > 0
Recommends:     coolercontrold = %{version}

Source:         https://gitlab.com/api/v4/projects/%{project_id}/packages/generic/%{name}/%{branch}/%{name}-%{branch}.tar.gz

%description
This is the desktop application for CoolerControl.
CoolerControl is an open-source application for monitoring and controlling supported cooling
devices. It features an intuitive interface, flexible control options, and live thermal data to keep
your system quiet, cool, and stable.

%prep
%autosetup -n %{name}-%{branch}/%{qt_dir} -a 0

%generate_buildrequires

%build
%cmake
%cmake_build

%install
%cmake_install
desktop-file-install --dir=%{buildroot}%{_datadir}/applications metadata/%{ap_id}.desktop
install -Dpm 644 metadata/%{ap_id}.svg -t %{buildroot}%{_datadir}/icons/hicolor/scalable/apps
install -Dpm 644 metadata/%{ap_id}-symbolic.svg -t %{buildroot}%{_datadir}/icons/hicolor/symbolic/apps
install -Dpm 644 metadata/%{ap_id}.png -t %{buildroot}%{_datadir}/icons/hicolor/256x256/apps
install -Dpm 644 metadata/%{ap_id}.metainfo.xml -t %{buildroot}%{_metainfodir}
install -Dpm 644 man/%{name}.1 -t %{buildroot}%{_mandir}/man1

%check
appstream-util validate-relax --nonet %{buildroot}%{_metainfodir}/*.metainfo.xml

%files
%{_bindir}/%{name}
%{_datadir}/applications/%{ap_id}.desktop
%{_datadir}/icons/hicolor/scalable/apps/%{ap_id}.svg
%{_datadir}/icons/hicolor/symbolic/apps/%{ap_id}-symbolic.svg
%{_datadir}/icons/hicolor/256x256/apps/%{ap_id}.png
%{_metainfodir}/%{ap_id}.metainfo.xml
%{_mandir}/man1/%{name}.1*
%license LICENSE
%doc CHANGELOG.md

%changelog
%autochangelog
