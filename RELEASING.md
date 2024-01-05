# CoolerControl Release Process

This is the basic template for how releases are done. Many of the standard tasks are handled using our Makefile and some
scripts. Each step must be completed successfully before moving on to the next one.

1. Update Changelog
    1. We use [this changelog format](https://keepachangelog.com/en/1.0.0/)
    2. add new version
    3. add needed subheadings: Added, Changed, Deprecated, Removed, Fixed, Security, Dependency Updates
    4. format changelog using .editorconfig
2. Update App Metadata
    1. Update `metadata/org.coolercontrol.CoolerControl.metainfo.xml`
        1. with new Release version and date
        2. plus any changes to screenshots and/or description
3. Update Packaging Settings
    1. Update `packaging/fedora/coolercontrol.spec`,`packaging/fedora/coolercontrold.spec`,`packaging/fedora/coolercontrol-liqctld.spec`
        1. Version near the top
        2. Changelog at the bottom
    2. Update `packaging/opensuse/coolercontrol.spec`
        1. Version near the top
        2. Changelog at the bottom
    3. Update `packaging/debian/changelog`
        1. Add whole new section at the beginning with top version and changelog entry
4. Create Release Tag and Commit and Build Release Artifacts
    1. Verify Milestone exists for the to-be-released version in GitLab
    2. All commits and tags are to be signed. Make sure your PGP keys are setup.
    3. In CoolerControl Repo run ```make release```
        1. without argument for a quick patch update 1.0.0 -> 1.0.1
        2. with 'v' argument to specifiy a version
            1. `v=minor`, `v=major`, `v=patch`
            2. for ex: ```make release v=minor``` 1.0.0 -> 1.1.0
        3. check the diff that the changes are correct
        4. `make push-release`
        5. make sure build & release pipelines complete successfully
        6. check that the release notes look correct (gitlab -> Deployment -> Releases)
        7. close the current Milestone
        8. create the next release milestone (can easily change the milestone name/version later)

5. AUR Release
    1. Currently, this is done mostly by hand (need to create friendly script at some point)
    2. Make sure SSH keys are setup correctly.
    3. cd to AUR Repo
    4. Adjust pkgver version number in `PKGBUILD`
    5. run `make clean` to clear the build dir
    6. run `make` - will fail with a validity check and if there are any missing dependencies you will have to install them manually
    7. run `make checksum` to get sha hash -> copy this into the `PKGBUILD`
    8. run `make clean` to clear the downloaded tar
    9. run `make` again. This time it should build & install the latest release package
    10. start daemon and quick test
    11. run `make clean` to clean out the build files
    12. push changes to AUR Repo with commit message as release version
    13. test that new version is available after a few minutes from Arch machine `yay -S coolercontrol`
