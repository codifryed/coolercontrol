# rpkg variant
%global _enable_debug_packages 0
%global debug_package %{nil}
%global project coolercontrol

Name:           %{project}-liqctld
Version:        2.2.2
Release:        1%{?dist}
Summary:        Monitor and control your cooling devices

License:        GPL-3.0-or-later
URL:            https://gitlab.com/%{project}/%{project}

BuildRequires:  systemd-rpm-macros
BuildRequires:  python3-devel
BuildRequires:  python3-wheel
BuildRequires:  python3-liquidctl
BuildRequires:  python3-setproctitle
BuildRequires:  python3-fastapi
BuildRequires:  python3-uvicorn

VCS:        {{{ git_dir_vcs }}}
Source:     {{{ git_dir_pack }}}

%description
CoolerControl is a program to monitor and control your cooling devices.

It offers an easy-to-use user interface with various control features and also provides live thermal performance details.

%prep
{{{ git_dir_setup_macro }}}

%generate_buildrequires
(cd %{name}; %pyproject_buildrequires)

%build
(cd %{name}; %pyproject_wheel)

%install
(cd %{name}; %pyproject_install)
(cd %{name}; %pyproject_save_files coolercontrol_liqctld)
mkdir -p %{buildroot}%{_unitdir}
cp -p packaging/systemd/%{name}.service %{buildroot}%{_unitdir}

%check
%pyproject_check_import

%files -f %{pyproject_files}
%{_bindir}/%{name}
%{_unitdir}/%{name}.service
%license LICENSE
%doc README.md CHANGELOG.md

%changelog
* Fri Jul 18 2025 Guy Boldon <gb@guyboldon.com> - 2.2.2-1
- 2.2.2 Release

* Fri Jun 13 2025 Guy Boldon <gb@guyboldon.com> - 2.2.1-1
- 2.2.1 Release

* Sat May 31 2025 Guy Boldon <gb@guyboldon.com> - 2.2.0-1
- 2.2.0 Release

* Sat Apr 05 2025 Guy Boldon <gb@guyboldon.com> - 2.1.0-1
- 2.1.0 Release

* Thu Mar 20 2025 Guy Boldon <gb@guyboldon.com> - 2.0.1-1
- 2.0.1 Release

* Sat Mar 15 2025 Guy Boldon <gb@guyboldon.com> - 2.0.0-1
- 2.0.0 Release

* Sat Dec 14 2024 Guy Boldon <gb@guyboldon.com> - 1.4.5-1
- 1.4.5 Release

* Sat Nov 02 2024 Guy Boldon <gb@guyboldon.com> - 1.4.4-1
- 1.4.4 Release

* Fri Nov 01 2024 Guy Boldon <gb@guyboldon.com> - 1.4.3-1
- 1.4.3 Release

* Fri Sep 27 2024 Guy Boldon <gb@guyboldon.com> - 1.4.2-1
- 1.4.2 Release

* Sat Sep 21 2024 Guy Boldon <gb@guyboldon.com> - 1.4.1-1
- 1.4.1 Release

* Sat Jul 27 2024 Guy Boldon <gb@guyboldon.com> - 1.4.0-1
- 1.4.0 Release

* Fri Jun 07 2024 Guy Boldon <gb@guyboldon.com> - 1.3.0-1
- 1.3.0 Release

* Sat Apr 13 2024 Guy Boldon <gb@guyboldon.com> - 1.2.2-1
- 1.2.2 Release

* Tue Apr 02 2024 Guy Boldon <gb@guyboldon.com> - 1.2.1-1
- 1.2.1 Release

* Mon Apr 01 2024 Guy Boldon <gb@guyboldon.com> - 1.2.0-1
- 1.2.0 Release

* Fri Feb 16 2024 Guy Boldon <gb@guyboldon.com> - 1.1.1-0
- 1.1.1 Release

* Wed Jan 31 2024 Guy Boldon <gb@guyboldon.com> - 1.1.0-0
- 1.1.0 Release

* Mon Jan 15 2024 Guy Boldon <gb@guyboldon.com> - 1.0.4-0
- 1.0.4 Release

* Mon Jan 15 2024 Guy Boldon <gb@guyboldon.com> - 1.0.3-0
- 1.0.3 Release

* Sun Jan 14 2024 Guy Boldon <gb@guyboldon.com> - 1.0.2-0
- 1.0.2 Release

* Fri Jan 12 2024 Guy Boldon <gb@guyboldon.com> - 1.0.1-0
- 1.0.1 Release

* Sun Jan 07 2024 Guy Boldon <gb@guyboldon.com> - 1.0.0-0
- 1.0.0 Release

* Fri Dec 15 2023 Guy Boldon <gb@guyboldon.com> - 0.17.3-0
- 0.17.3 Release

* Tue Nov 28 2023 Guy Boldon <gb@guyboldon.com> - 0.17.2-0
- 0.17.2 Release

* Wed Sep 13 2023 Guy Boldon <gb@guyboldon.com> - 0.17.1-0
- 0.17.1 Release

* Sun Jul 16 2023 Guy Boldon <gb@guyboldon.com> - 0.17.0-0
- 0.17.0 Release

* Sun Apr 23 2023 Guy Boldon <gb@guyboldon.com> - 0.16.0-0
- 0.16.0 Release

* Tue Mar 14 2023 Guy Boldon <gb@guyboldon.com> - 0.15.0-0
- 0.15.0 Release

* Wed Mar 01 2023 Guy Boldon <gb@guyboldon.com> - 0.14.6-0
- 0.14.6 Release

* Mon Feb 27 2023 Guy Boldon <gb@guyboldon.com> - 0.14.5-0
- 0.14.5 Release

* Tue Feb 14 2023 Guy Boldon <gb@guyboldon.com> - 0.14.4-0
- 0.14.4 Release

* Thu Feb 09 2023 Guy Boldon <gb@guyboldon.com> - 0.14.3-0
- 0.14.3 Release

* Tue Feb 07 2023 Guy Boldon <gb@guyboldon.com> - 0.14.2-0
- 0.14.2 Release

* Mon Feb 06 2023 Guy Boldon <gb@guyboldon.com> - 0.14.1-0
- 0.14.1 Release

* Sun Feb 05 2023 Guy Boldon <gb@guyboldon.com> - 0.14.0-0
- 0.14.0 Release
