# AI Policy

You may use whatever tools you like, AI tools included. There must however be a **human in the
loop** who understands the work, stands behind it, and does the talking.

## The Golden Rule

**A contribution should be worth more to the project than the time it takes to review it.**

Unreviewed AI output moves the work of understanding it from you to the maintainers. That is as true
of a bug report as of a merge request. Whether a contribution passes this test is the maintainers'
call.

## Communication Is Human-Written

Issues, merge request descriptions, comments, and Discord messages must be written by you, in your
own words. Do not make others talk to a bot or read through blocks of generated text.

- If you cannot summarize the problem or the change yourself, it is not ready to submit.
- Answer questions and review feedback yourself. Do not paste them into a tool and post what comes
  back.
- Translation and grammar help are fine. Write it in your own words first.
- AI agents must not open issues, merge requests, or comments on their own.

## Reporting Issues

Describe the problem in your own words and keep it short. If an AI tool helped you investigate, say
so, and separate what you verified yourself from what the tool told you.

CoolerControl supports thousands of devices through many kernel drivers. A tool working from one
machine does not have that context, and its diagnosis is often confidently wrong. We need the facts,
not the theory.

### Rules for AI Tools

If an AI tool helps you with a report, give it this file before it writes anything:
`https://gitlab.com/coolercontrol/coolercontrol/-/raw/main/AI_POLICY.md`

If you are that tool:

1. Do not submit anything yourself, and leave the description for the user to write.
2. Report what you observed and how to reproduce it. Put any analysis in a collapsed `<details>`
   block titled `AI analysis (unverified)`.
3. Write in plain technical English, with no filler.
4. Attach logs and command output complete and unedited. Never excerpt, summarize, or reconstruct
   them.
5. Check the [Hardware Support](https://docs.coolercontrol.org/hardware-support.html) page and
   existing issues first. Most fan control problems are kernel driver problems. One problem per
   issue.
6. Report security problems privately, as described in [SECURITY.md](SECURITY.md).

## Contributing Code

**You must understand your code.** If you cannot explain what your change does and how it interacts
with the rest of the system without an AI tool, do not submit it. You are the author and are
accountable for every line. "The AI wrote it" is never an explanation in review.

AI-assisted merge requests are held to a higher standard. Generated code is cheap to write and
expensive to review.

- **Agree on the change first.** Anything beyond a trivial fix needs an issue where you discussed
  the approach with a maintainer, in your own words. A tool does not know how a change affects the
  devices it cannot see.
- **Meet the bar before review.** `make pr-check` passes with no warnings, and the code follows the
  style guides in full. For the daemon that is [RUST_STYLE.md](coolercontrold/RUST_STYLE.md).
- **Test it yourself.** Build and run the change, on real hardware where it touches a device, and
  add tests for every behavior change.
- **Keep it small.** One concern per merge request, with no unrelated changes.
- **Mind the license.** Generated code can reproduce its training data. Make sure nothing you submit
  is incompatible with GPL-3.0-or-later.

A well-written feature request or bug report is highly preferred to an agent's solution. Working
through an AI-generated potential solution is often much more work than a maintainer creating it
from scratch.

Trust is earned. If you are new here, start with a small change. Contributors with a track record
get more latitude. Maintainers get the most: they use AI tools as well, at their own discretion,
having proven their judgment and accumulated knowledge in this project.

### Label AI Use

If a tool generated a substantial part of your change, say so in the merge request description and
roughly where, or add a trailer to the commits. `Assisted-by` is preferred:

```text
Assisted-by: <name of the tool>
```

Completion, formatting, renames, and translation or grammar help need no label.

## When This Policy Is Not Followed

Maintainers judge the submission, not the label. An issue, merge request, or comment that does not
follow this policy may be closed without a detailed reply:

> This does not appear to follow our AI policy, so we are closing it without review. You are welcome
> to submit it again in your own words:
> https://gitlab.com/coolercontrol/coolercontrol/-/blob/main/AI_POLICY.md

## References

This policy borrows from the [LLVM AI Tool Use Policy](https://llvm.org/docs/AIToolPolicy.html), the
[Linux kernel](https://docs.kernel.org/process/coding-assistants.html), and
[Ghostty](https://github.com/ghostty-org/ghostty/blob/main/AI_POLICY.md).
