# CoolerControl Release Process

This is the basic template for how releases are done. Many of the standard tasks are handled using
our Makefile and some scripts. Each step must be completed successfully before moving on to the next
one.

Note: Steps 2 and 3 have been replaced by an improved `make bump` script, which handles these steps
automatically. They are included here for reference:

1. Update Changelog
   1. We use [this changelog format](https://keepachangelog.com/en/1.0.0/)
   2. Add a new version
   3. Add necessary subheadings: Added, Changed, Deprecated, Removed, Fixed, Security, Dependency
      Updates
   4. Format changelog appropriately

2. Update App Metadata (replaced with script)
   1. Update `metadata/org.coolercontrol.CoolerControl.metainfo.xml`
      1. with new Release version and date
      2. plus any changes to screenshots and/or description

3. Update Packaging Settings (replaced with `make bump` target)
   1. Update `packaging/fedora/coolercontrol.spec`,`packaging/fedora/coolercontrold.spec`
      1. Version near the top
      2. Changelog at the bottom
   2. Update `packaging/fedora/coolercontrol-rc1.spec`,`packaging/fedora/coolercontrold-rc1.spec`
      1. Use the 'next' patch version.
   3. Update `packaging/debian/changelog`
      1. Add a whole new section at the beginning with top version and changelog entry

4. Create Release Tag and Commit and Build Release Artifacts
   1. Verify Milestone exists for the to-be-released version in GitLab
   2. All commits and tags are to be signed. Make sure your PGP keys are setup.
   3. Check `DEPRECATIONS.md`: remove any shim whose Remove version is at or below the version being
      released, along with its registry row.
   4. In CoolerControl Repo root directory
      1. IF version needs to be increase again (from post-release) run `make bump`
      2. without argument for a quick patch update 1.0.0 -> 1.0.1
      3. with 'v' argument to specify a version
         1. `v=minor`, `v=major`, `v=patch`
         2. for ex: `make bump v=minor` 1.0.0 -> 1.1.0
      4. run `make release`
      5. check the diff that the changes are correct
      6. `make push-release`. It pushes the Release commit alone, waits for that commit's release
         rehearsal to pass, then pushes the tag. The rehearsal runs every check and build without
         publishing: the test suite on old and new distros for amd64 and arm64 (the newest with the
         newest stable Rust, warnings denied), the Fedora RPM specs in mock, and every package and
         Docker image, which its jobs keep for the tag. A rehearsal is a push or web pipeline of a
         `Release X.Y.Z` commit on the default branch whose `prerelease_passed` job, which needs
         every other job, passed. Only the protected default branch can produce one, and only a job
         can write its own artifacts, so what the rehearsal's jobs hold is exactly what was tested.
         The tag pipeline first verifies that rehearsal through the API (`verify_prerelease`), then
         fetches its jobs' builds instead of rebuilding and publishes them; Docker images are staged
         under a commit-keyed tag and given their release tags by digest. A tag never reaches
         downstream packagers before its commit has passed. `make prerelease-local` runs the tests
         and RPM builds on this machine beforehand. `make push-release` needs `GITLAB_TOKEN`, a
         read_api token, to watch the rehearsal; the tag pipeline checks it with the masked,
         protected CI variable `RELEASE_READ_TOKEN` (tags must be protected for it to reach their
         pipelines). Make that a project access token of this project, not a personal one: Reporter
         role, `read_api` scope only, with an expiry date. Every main and tag job can read it.
         - If the rehearsal fails, the tag has not left this machine. Delete it
           (`git tag -d X.Y.Z`), push the fix, then `make release` (an empty Release commit) and
           `make push-release` again.
      7. make sure the tag pipeline completes successfully. It only checks that its commit was
         rehearsed, then publishes the rehearsed builds, so a failed job can be retried without
         retagging. The rehearsal's builds expire 5 days after it ran: retry before then, or the
         release has to be rehearsed again.
      8. check that the release notes look correct (gitlab -> Deployment -> Releases)
      9. close the current Milestone
      10. create the next release milestone (can easily change the milestone name/version later)
   5. Apt Repo: Verify amd and arm package uploads were successful.
   6. DockerHub: Adjust Image links on main page
   7. Copr: Adjust git tag version for each package and trigger new builds.
   8. Merge any website changes to be made.

5. Update OpenAPI specification
   1. Nothing to do: `make bump` regenerates `openapi/openapi.json` (the spec stamps the daemon
      version) and `make release` commits it. A daemon test fails when the file is stale.
   2. To regenerate it by hand at any time, run `make openapi`. It needs no running daemon and no
      root, since the spec is generated from the daemon's route table.
   3. Update Website with new `openapi.spec` file.

6. NixOS Release
   1. You need Nix package manager setup on your system.
   2. Update the fork of the repo: https://github.com/NixOS/nixpkgs
   3. `cd nixpkgs`
   4. Create new branch: `coolercontrol.*`
   5. Edit the release version: `vi pkgs/applications/system/coolercontrol/default.nix` and remove
      the hash from that file to force a new source download
   6. (If not NixOS) Setup the store:
      - `sudo mkdir -p /nix`
      - `sudo mount --bind ~/.local/share/nix/root/nix /nix`
   7. Build the packages, replacing the Hashes where appropriate:
      `nix --extra-experimental-features nix-command build -f . coolercontrol -L`
      `nixpkgs-review wip`
   8. Run the built binaries (in a root-less install):
      - `sudo ./result-1/bin/coolercontrold`
      - `./result/bin/coolercontrol --disable-gpu` -- doesn't work on a non-native NixOS
   9. Commit changes with message: `coolercontrol: 4.1.0 -> 4.3.0`
   10. Open a PR at https://github.com/NixOS/nixpkgs/ `coolercontrol: 4.0.1 -> 4.1.0`
   11. Note: if you ever need to fix the local nix store: To rebuild ignoring the binary cache for
       that derivation: `--rebuild`. To force a fresh evaluation: `--no-eval-cache`.
       `sudo nix-store --verify --check-contents --repair` or to clear out garbage:
       `nix-collect-garbage -d`

7. Post Release
   1. Run `make bump`
   2. without argument for a quick patch update 1.0.0 -> 1.0.1
   3. After a release we just want to at least bump the source version by a patch number, for
      clarity when testing. We can increase it later for the next release.
   4. Update all depdencies - allows us a good amount of testing before the next release.

<!--Test-->
