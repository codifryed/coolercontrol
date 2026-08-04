# CoolerControl apt repository

Tooling for the self-managed apt repository at `https://apt.coolercontrol.org`, an aptly archive
stored in a Cloudflare R2 bucket and published from CI.

## Layout

```text
apt.coolercontrol.org/
  setup.sh                            distro detecting installer
  coolercontrol-archive-keyring.gpg   binary public keyring
  coolercontrol.asc                   armored copy of the same key
  debian/{dists,pool}                 bookworm builds, Suite stable, amd64 + arm64
  ubuntu/{dists,pool}                 jammy builds, Suite stable, amd64 + arm64
  locks/publish.lock                  transient, held during a publish
```

Two separate trees rather than two suites in one: the bookworm-built and jammy-built
`coolercontrold_<version>_amd64.deb` have identical file names with different contents, so one pool
cannot hold both. Path prefixes are the same approach download.docker.com uses.

Both trees serve `amd64` and `arm64`. Architecture never affects which tree a user gets.

**The published pool is the only state.** Every publish pulls it, rebuilds the index from it plus
the new packages, and pushes the result. There is no database in the bucket to keep in sync, lose or
repair, which means the recovery path and the normal path are the same code, exercised on every run.

## Which tree a user gets

`setup.sh` and the `coolercontrold` postinst apply the same mapping, by distribution only:

| Distribution                                                | Tree      |
| ----------------------------------------------------------- | --------- |
| Debian                                                      | `/debian` |
| Linux Mint Debian Edition (`ID_LIKE=debian`)                | `/debian` |
| any other Debian derivative that is not Ubuntu based        | `/debian` |
| Ubuntu, Pop!\_OS, Linux Mint, elementary, Kali, Zorin, Neon | `/ubuntu` |
| anything unrecognised                                       | `/ubuntu` |

Kali needs its explicit entry: it reports `ID_LIKE=debian` but has always been served the Ubuntu
build. Two deliberate differences from the old Cloudsmith routing: the `otherdeb` bucket used to
receive Ubuntu builds and now maps to the Debian tree, and the seven cosmetic Cloudsmith buckets
collapse into these two trees.

Architecture independent packages (the DKMS modules, liquidctl) are published to both trees.

## Publishing

The publish script is baked into `registry.gitlab.com/coolercontrol/coolercontrol/apt-publish` as
`/usr/local/bin/apt-repo-publish`, together with the keyring and `setup.sh`.

```sh
apt-repo-publish --repo debian|ubuntu|both [--keep N] <deb>...
```

It needs `GPG_KEY`, `GPG_PHRASE`, `R2_ACCOUNT_ID`, `R2_BUCKET`, `R2_ACCESS_KEY_ID` and
`R2_SECRET_ACCESS_KEY`, all inherited from GitLab group level variables. Set `APT_REPO_DEST` to a
local directory to publish against a stand-in bucket without credentials.

Per tree it takes the publish lock, pulls the published pool, rebuilds an aptly repository from
those packages plus the new ones, applies retention, signs and publishes, then pushes in three
steps:

1. `rclone copy` the pool, so new files exist before the metadata names them.
2. `rclone sync` `dists/`, so the metadata names only files that exist.
3. `rclone sync` the pool, so retired files disappear only after nothing names them.

Step 3 deletes, so the script refuses to run it unless the freshly published pool holds at least as
many packages as were passed in.

Re-running a publish with the same packages is a no-op, so a failed job can simply be retried.

### Version retention

`--keep N` (default 3) keeps the newest N versions of each package installable and drops the rest.
That is what makes a rollback possible:

```sh
apt install coolercontrold=4.3.1
```

Without it a user who hits a regression has no way back except downloading a `.deb` by hand.
Retention also bounds the pool: each release adds roughly 20 MB per tree, so keeping three holds
each tree near 60 MB, which is what a publish pulls.

Versions are ordered with `dpkg --compare-versions`, not `sort -V`, which gets epochs and `~`
suffixes wrong. Note that Debian treats `<` as `<=` in package queries, so the script removes by
exact version rather than by range.

### Cache behaviour

Cloudflare cache rules bypass the cache for `*/dists/*`, `/setup.sh`, and the two key files, and
cache `*/pool/*` for 30 days (pool files are immutable by name). Because `dists/` is never cached,
publishing needs no cache purge.

Uploads carry `Cache-Control: ... no-transform`. Cloudflare transparently decompresses gzipped
objects, which would make the served `Packages.gz` disagree with the hash recorded in `Release` and
break `apt update` with a hash sum mismatch. Confirm this specifically when the bucket first goes
live.

### Concurrency

Publishers are the main repository's tag pipeline plus the weekly scheduled pipelines of the sibling
package repositories. They coordinate through `locks/publish.lock`: a fresh lock is waited on for up
to 10 minutes, a lock older than 30 minutes is treated as stale and taken over.

This is best effort, not a correctness guarantee. Two publishers starting in the same second can
both proceed. It is acceptable because the schedules are staggered and any tree can be rebuilt from
its own pool, but keep new scheduled publishers staggered against the existing ones.

## Seeding a new bucket

1. Collect the debs to seed: `coolercontrol` and `coolercontrold` for the releases you want
   available from the GitLab release pages, plus the latest `it87-dkms`, `nct6687d-dkms` and
   `liquidctl` debs from their own releases. Seed the current stable release before anything newer
   is published, so users have a rollback target from day one.
2. Publish them. Order does not matter: retention keeps the newest versions whatever order they
   arrive in.

   ```sh
   apt-repo-publish --repo debian  <bookworm debs>
   apt-repo-publish --repo ubuntu  <ubuntu debs>
   apt-repo-publish --repo both    <arch all debs>
   ```

   The `_bookworm` and `_ubuntu` suffixes in the release download names do not matter: pool names
   come from the control metadata, so they normalise automatically.

3. Verify from clean containers, on both `debian:bookworm` and `ubuntu:jammy`, with only the shipped
   keyring trusted:

   ```sh
   curl -fsSL https://apt.coolercontrol.org/setup.sh | sh
   apt-get install coolercontrold
   apt-cache policy coolercontrold      # every retained version should be listed
   ```

   Zero apt trust warnings is the pass condition. Repeat on an arm64 container to confirm both trees
   serve both architectures.

## Recovery

- **A publish failed part way.** Re-run the job.
- **The metadata is wrong or missing.** Re-run any publish. The index is rebuilt from the pool every
  time, so a single successful run repairs it.
- **The pool itself is lost.** Re-publish the wanted versions from the GitLab release pages. The
  release assets are the durable copy of every shipped `.deb`.
- **The signing key is compromised.** There is no way around every user importing a new key. The key
  is `21B8 9EAE A5DA C4FB C7C0 9333 6681 89E5 007F 5A8D`, reused from Cloudsmith so that no user had
  to import anything during the migration.
