# Tempest maintenance fork

Upstream: https://github.com/joske/yserver

Maintenance branch: `codex/portable-embedded-core`.

This branch merges upstream history normally. Do not periodically copy source,
rebase published tags, or carry a second copy of protocol/request dispatch.
Native and Tempest transports share the same reader, scheduler and handlers, so
upstream fixes to those paths apply to both configurations.

## Local changes

- `yserver-core` retains upstream's default Unix implementation behind the
  default `native` feature. Platform imports, FD contracts and shared memory are
  gated so hosts can opt out without stubbing platform APIs.
- The portable core provides the in-process transport. Its implementation and
  focused tests live in `memory_stream.rs` and `memory_stream/tests.rs`.
- `host_x11/types.rs` contains the existing portable input/window data, shared
  by the native host backend and the portable module. It is not a second backend.
- The core loop accepts transport wakeups and excludes FD poll registration for
  memory clients. Protocol decoding and request dispatch stay shared with upstream.
- Native tests are retained. Portable hosts use memory-stream tests and downstream
  Tempest tests of rendering, input, authentication, isolation and shutdown.
- Fork CI also runs on maintenance-branch pushes, without requiring a PR.

Tempest's application-specific rendering/backend remains in its `tempest-x11`
crate, outside this repository. This fork is maintained for Tempest; an upstream
PR is not part of the current workflow.

## Merge an upstream update

Start from a clean checkout of the maintenance branch:

```sh
git remote add upstream https://github.com/joske/yserver.git # once
git switch codex/portable-embedded-core
git fetch upstream
git merge upstream/master
```

Keep upstream's behavior in the default native path. If upstream touched a gated
section, resolve the conflict by applying its logic to the shared code, preserving
only the platform boundary. In particular, do not create a separate request reader
or dispatcher to avoid a merge conflict. Git can merge disjoint edits automatically;
overlapping edits still need review.

Run the checks in `crates/yserver-core/README.md`, plus the upstream full-workspace
checks on Linux with the dependencies in `docs/setup.md`. Push the branch so both
native CI and Windows/macOS/Linux portable CI run. Cross-check Android too. Then
validate the dependent Tempest X11 tests against the candidate commit before
cutting an immutable `yserver-core-v<version>-tempest.<revision>` tag. Update both
Tempest Git dependencies and Cargo.lock together. Never move an existing tag.

The first maintenance commit is based on upstream
`7fef146b9b8d64003336cd065cfb1807e2b7735a` (yserver 1.7.1). Later upstream merges
remain visible in Git history and can be inspected with `git log --first-parent`.
