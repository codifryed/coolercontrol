# Contributing to CoolerControl

A big welcome and thank you for considering contributing to CoolerControl! It’s people like you that
make it a reality for users in our community. :heart:

Reading and following these guidelines will help us make the contribution process easy and effective
for everyone involved. It also communicates that you agree to respect the time of the developers
managing and developing this open source project. In return, we will reciprocate that respect by
addressing your issue, assessing changes, and helping you finalize your pull requests.

## Quicklinks

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
  - [Issues](#issues)
  - [Merge Requests](#merge-requests)
    - [Development Environment](#development-environment)
- [Getting Help](#getting-help)

## Code of Conduct

We take our open source community seriously and hold ourselves and other contributors to high
standards of communication. By participating and contributing to this project, you agree to uphold
our
[Code of Conduct](https://gitlab.com/coolercontrol/coolercontrol/-/blob/main/CODE_OF_CONDUCT.md).

## Getting Started

Contributions are made to this repo via Issues and Merge Requests (MRs). A few general guidelines
that cover both:

- Search for existing [Issues](https://gitlab.com/coolercontrol/coolercontrol/-/issues) and
  [MRs](https://gitlab.com/coolercontrol/coolercontrol/-/merge_requests) before creating your own.
- We work hard to makes sure issues are handled in a timely manner but, depending on the impact, it
  could take a while to investigate the root cause. A friendly ping in the comment thread to the
  submitter or a contributor can help draw attention if your issue is blocking.
- If you've never contributed before, checkout this
  [how to contribute guide](https://opensource.guide/how-to-contribute/) for resources and tips on
  how to get started.

### Issues

Issues should be used to report problems with the application, request a new feature, or to discuss
potential changes before a PR is created. When you create a new Issue, choosing a Bug or Feature
Request template will help guide you through collecting and providing the information we need to
investigate.

If you find an Issue that addresses the problem you're having, please add your own reproduction
information to the existing issue rather than creating a new one. Adding a
[reaction](https://github.blog/2016-03-10-add-reactions-to-pull-requests-issues-and-comments/) can
also help indicate to our maintainers that a particular issue is affecting more than just the
reporter.

### Merge Requests

MRs to CoolerControl are always welcome and can be a quick way to get your fix or improvement slated
for the next release. If you're new to merge requests, checkout
[getting started with merge requests.](https://docs.gitlab.com/ee/user/project/merge_requests/getting_started.html)
In general, MRs should:

- Only fix/add the functionality in question **OR** address wide-spread whitespace/style issues, not
  both.
- Add unit or integration tests for fixed or changed functionality (if a test suite already exists).
- Address a single concern in the least number of changed lines as possible.
- Include documentation in the repo or in the
  [readme](https://gitlab.com/coolercontrol/coolercontrol/-/blob/main/README.md).
- Be accompanied by a complete Merge Request template (loaded automatically when an MR is created).

For changes that address core functionality, affect the main features, or would require breaking
changes (e.g. a major release), it's best to open an Issue to discuss your proposal first. This can
save time creating and reviewing changes. You can also reach out to us and other fellow contributors
on [Discord](https://discord.gg/MbcgUFAfhV).

In general, we follow the
[project forking workflow](https://docs.gitlab.com/ee/user/project/repository/forking_workflow.html)

1. Fork the repository to your own GitLab account
2. Clone the project to your machine
3. Create a branch locally with a succinct but descriptive name
4. Commit changes to the branch
5. Following any formatting and testing guidelines specific to this repo
6. Push changes to your fork
7. Open an MR in our repository and follow the MR template so that we can efficiently review the
   changes.

#### Development Environment

We supply a nix-shell configuration to quickly set up an environment containing all the necessities
for CoolerControl development.

To use it, make sure you have [Nix installed](https://nixos.org/download/#nix-install-linux).

Then run `nix-shell /path/to/coolercontrol/shell.nix`. This will give a shell without modifying your
system.

## Getting Help

Join out our [CoolerControl Discord channel](https://discord.gg/MbcgUFAfhV) and post your question
there. You may get a response from a community manner or from our maintainers in a timely manner.
