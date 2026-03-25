### Build

- **Faster pre-push checks**: updated `.githooks/pre-push` to run changed-files scoped frontend and Rust checks by default.
- **Optional full gate**: full pre-push validation can still be forced with `PREPUSH_FULL=1 git push`.
