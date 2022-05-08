# Coolero Release Process

This is the basic template for how releases are done. Many of the standard tasks are handled using our Makefile and some
scripts. Each step must be completed successfully before moving on to the next one.

1. Update Changelog
    1. We use [this changelog format](https://keepachangelog.com/en/1.0.0/)
    2. add new version
    3. add needed subheadings: Added, Fixed, and Changed
    4. format changelog using .editorconfig
2. Update App Metadata
    1. Update `metadata/org.coolero.Coolero.metainfo.xml`
        1. with new Release version and date
        2. plus any changes to screenshots and/or description
3. Appimage Testing
    1. run `make` in root dir (see AppImage Release step below if running for the first time)
    2. run `./Coolero-x86_64.AppImage` and make sure everything works as expected
4. Flatpak Testing
    1. Make sure the Flatpak repo and Coolero repo share the same parent directory!
    2. If there __ARE__ dependency changes:
        1. in coolero repo:
            1. `make flatpak-export-deps`
        2. in flatpak repo:
            1. edit requirements.txt to exclude: nuitka, pyamdgpuinfo, pyside6, shiboken6
            2. make sure the only changes to dependencies are expected
            3. `make deps`
            4. verify py-dependencies.yaml by hand
                1. put changes in the correct places, matplotlib always at the end with special handling
                2. undo removal of special handling for some packages (patches for flatpak building)
            5. continue with No Dependency changes steps:
    3. Else if there are __NO__ dependency changes:
        1. goto the Flatpak Repo
        2. comment out tag and commit id for the coolero repo in `org.coolero.Coolero.yaml`
        3. add `branch: main` - so that we can test the current state of the main branch
        4. comment out `tag:` and `commit:`
        5. `make`
        6. take a break while it runs and builds (subsequent builds are much faster)
        7. run the installed flatpak `flatpak run org.coolero.Coolero` to make sure everything works as expected
5. Create Release Tag and Commit
    1. Verify Milestone exists for the to-be-released version in GitLab
    2. All commits and tags are to be signed. Make sure your PGP keys are setup.
    3. In Coolero Repo run ```make release```
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

##### The following steps can be done somewhat in parallel:

6. AppImage Release
    1. docker is required (in future will have pipeline to do this)
    2. make sure correct PGP keys are installed (images are signed)
    3. make sure your GitLab access token is set in the env variable: `COOLERO_TOKEN`
    4. cd to the Coolero Repo
    5. if this is the first time on this machine: `make docker-build-images`
    6. `make` - compiles and builds the AppImage
        1. for convenience the build will pause before asking for the pgp key password for signing
    7. `make push-appimage` - pushes the build image and sync file to the GitLab package repository
    8. can test a successful upload by using the 'check for updates' AppImage setting
7. Flatpak Release
    1. goto Flatpak Repo
    2. in `org.coolero.Coolero.yaml`
        1. remove branch entry and update with new tag and commit hash
    3. commit changes to dev branch and push to Flathub repo on GitHub
    4. open a PR in GitHub to merge master <- dev
        1. wait for the Flathub test build to successfully finish (~30 min)
        2. remove any locally installed flatpak `flatpak remove org.coolero.Coolero`
        3. test the Flathub build by using the link provided in the PR (without `--user`)
        4. if everything works as expected, merge the PR into master
    5. The official release build will be published in about 3 hours after changes to master.
8. AUR Release
    1. Currently, this is done mostly by hand (need to create friendly script at some point)
    2. Make sure SSH keys are setup correctly. 
    3. cd to AUR Repo
    4. Adjust version number, source hashes, and tarball filename of `PKGBUILD` and `.SRCINFO`
        1. get release tarball `wget https://gitlab.com/coolero/coolero/-/archive/0.10.2/coolero-0.10.2.tar.gz`
        2. get hash `sha256sum ./coolero-0.10.2.tar.gz`
        3. repeat above at least 2x to make sure sha is correct (GitLab does not currently generate these)
    5. Arch Local Testing
        1. important if there are dependency changes or larger updates - small changes not so much
        2. run `makepkg -sfi` from AUR Repo folder
            1. will create a lot of files and first time is a bit tricky
            2. needs dependencies already installed
        3. run `makepkg --printsrcinfo > .SRCINFO`
    6. push changes to AUR Repo with commit as release version
    7. test changes work live from Arch machine `yay -S coolero`
9. Post link to GitLab release notes in discord #release channel