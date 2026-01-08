# CoolerControl Release Process

This is the basic template for how releases are done. Many of the standard tasks are handled using
our Makefile and some scripts. Each step must be completed successfully before moving on to the next
one.

1. Update Changelog
   1. We use [this changelog format](https://keepachangelog.com/en/1.0.0/)
   2. Add a new version
   3. Add necessary subheadings: Added, Changed, Deprecated, Removed, Fixed, Security, Dependency
      Updates
   4. Format changelog appropriately

2. Update App Metadata
   1. Update `metadata/org.coolercontrol.CoolerControl.metainfo.xml`
      1. with new Release version and date
      2. plus any changes to screenshots and/or description

3. Update Packaging Settings
   1. Update `packaging/fedora/coolercontrol.spec`,`packaging/fedora/coolercontrold.spec`
      1. Version near the top
      2. Changelog at the bottom
   2. Update `packaging/fedora/coolercontrol-rc1.spec`,`packaging/fedora/coolercontrold-rc1.spec`
   3. Update `packaging/debian/changelog`
      1. Add a whole new section at the beginning with top version and changelog entry
   4. Update `packaging/obs/coolercontrol.dsc`
      1. Version near the top
      2. Debtransform-Tar filename (to match gitlab release filename)
      3. File size and checksums are ignored
      4. If you need to generate a new .dsc file (eg new dependencies added to `debian/control`), from the root of the tree:
         - `dpkg-source -D"Debtransform-Tar: coolercontrol-$VERSION.tar.gz" -D"Debtransform-Files: coolercontrold-vendor.tar.gz" -D"Debtransform-Files-Tar: debian.tar.gz" -b .`
         - `sed -i -E 's/native/quilt/' ../*.dsc`

4. Create Release Tag and Commit and Build Release Artifacts
   1. Verify Milestone exists for the to-be-released version in GitLab
   2. All commits and tags are to be signed. Make sure your PGP keys are setup.
   3. In CoolerControl Repo root directory
      1. IF version needs to be increase again (from post-release) run `make bump`
      2. without argument for a quick patch update 1.0.0 -> 1.0.1
      3. with 'v' argument to specify a version
         1. `v=minor`, `v=major`, `v=patch`
         2. for ex: `make bump v=minor` 1.0.0 -> 1.1.0
      4. run `make release`
      5. check the diff that the changes are correct
      6. `make push-release`
      7. make sure build & release pipelines complete successfully
      8. check that the release notes look correct (gitlab -> Deployment -> Releases)
      9. close the current Milestone
      10. create the next release milestone (can easily change the milestone name/version later)
   4. Cloudsmith: Verify amd and arm package uploads are all present and add any missing ones.
   5. DockerHub: Adjust Image links on main page
   6. Copr: Adjust git tag version for each package and trigger new builds.
   7. Merge any website changes to be made.

5. Update OpenAPI specification
   1. Update locally running daemon with a new version: `make dev-build && make dev-install`
   2. Run `cd openapi;./update.sh`
   3. Commit a new file to repo.
   4. Update Website with new `openapi.spec` file.

6. NixOS Release
   1. You need Nix package manager setup on your system.
   2. Update the fork of the repo: https://github.com/NixOS/nixpkgs
   3. `cd nixpkgs`
   4. Create new branch: `coolercontrol.*`
   5. Edit the release version: `vi pkgs/applications/system/coolercontrol/default.nix` and remove
      the hash from that file to force a new source download
   6. Copy the new Tauri Cargo.lock:
      `cp ../coolercontrol/coolercontrold/Cargo.lock pkgs/applications/system/coolercontrol/Cargo.lock`
   7. Build the packages, replacing the Hashes where appropriate: `nix-build -A coolercontrol`
   8. Commit changes with message: `coolercontrol.*: 1.1.1 -> 1.2.0`
   9. Open a PR at https://github.com/NixOS/nixpkgs/
   10. Note: if you ever need to fix the local nix store:
       `sudo nix-store --verify --check-contents --repair` or to clear out garbage:
       `nix-collect-garbage -d`

7. Post Release
   1. Run `make bump`
   2. without argument for a quick patch update 1.0.0 -> 1.0.1
   3. After a release we just want to at least bump the source version by a patch number, for
      clarity when testing. We can increase it later for the next release.
   4. Update all depdencies - allows us a good amount of testing before the next release.

<!--Test-->
