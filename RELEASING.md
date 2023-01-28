# CoolerControl Release Process

This is the basic template for how releases are done. Many of the standard tasks are handled using our Makefile and some
scripts. Each step must be completed successfully before moving on to the next one.

1. Update Changelog
    1. We use [this changelog format](https://keepachangelog.com/en/1.0.0/)
    2. add new version
    3. add needed subheadings: Added, Changed, Deprecated, Removed, Fixed, Security
    4. format changelog using .editorconfig
2. Update App Metadata
    1. Update `metadata/org.coolercontrol.CoolerControl.metainfo.xml`
        1. with new Release version and date
        2. plus any changes to screenshots and/or description
3. Update Packageing Settings
    1. Update `packaging/fedora/coolercontrol.spec`
        1. Version near the top
        2. Changelog at the bottom
    2. Update `packaging/debian/changelog`
        1. Add new top version and changelog entry
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
    4. Adjust version number, source hashes, and tarball filename of `PKGBUILD` and `.SRCINFO`
        1. get release tarball `wget https://gitlab.com/coolercontrol/coolercontrol/-/archive/0.14.0/coolercontrol-0.14.0.tar.gz`
        2. get hash `sha256sum ./coolercontrol-0.14.0.tar.gz`
        3. repeat above at least 2x to make sure sha is correct (GitLab does not currently generate these)
    5. Arch Local Testing
        1. important if there are dependency changes or larger updates - small changes not so much
        2. run `makepkg -sfi` from AUR Repo folder
            1. will create a lot of files and first time is a bit tricky
            2. needs dependencies already installed
        3. run `makepkg --printsrcinfo > .SRCINFO`
    6. push changes to AUR Repo with commit as release version
    7. test changes work live from Arch machine `yay -S coolercontrol`