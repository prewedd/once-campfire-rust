# How to contribute to Campfire in Rust

This repository uses GitHub
[discussions](https://github.com/basecamp/once-campfire-rust/discussions) to track feature
requests and questions, rather than [the issue
tracker](https://github.com/basecamp/once-campfire-rust/issues). If you're considering opening an
issue or pull request, please open a discussion instead.

Whenever a discussion leads to an actionable and well-understood task, we'll move it to the issue
tracker where it can be worked on.

## What this means in practice

### If you'd like to contribute to the code...

1. If you're interested in working on one of the open issues, please do! We are grateful for the
   help!
2. Make sure someone else isn't already working on the same issue. If they are, it will be tagged
   "in progress" and/or it should be clear from the comments. When in doubt, comment on the issue
   to ask.
3. Read [`AGENTS.md`](AGENTS.md) for the layout and working rules: how to build and test, what has
   to stay compatible with existing installs, and where deliberate differences from the Rails app
   are recorded.
4. When you have something ready for review or collaboration, run `cargo fmt --all` and open a PR.
   CI checks the formatting and runs clippy and the tests; changes to what pages render should also
   pass the parity gate (`parity/bin/candidate compare`, see the README).

### If you've found a bug...

1. If you don't have steps to reproduce the problem, or you're not certain it's a bug, open a
   discussion.
2. If you have steps to reproduce, open an issue. If it's a security issue, see
   [`SECURITY.md`](SECURITY.md) instead.

### If you have an idea for a feature...

1. Open a discussion. Campfire's features come from the Rails app
   ([basecamp/once-campfire](https://github.com/basecamp/once-campfire)); this repository is about
   running them faster and leaner.

### If you have a question, or are having trouble with configuration...

1. Open a discussion.

Thanks for helping! ❤️
