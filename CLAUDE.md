# Altered Arena

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/) for every commit message and
every PR title (PRs are squash-merged with the PR title as the commit message):

- `feat: ...` a new feature (minor version)
- `fix: ...` a bug fix (patch version)
- `feat!: ...` or a `BREAKING CHANGE:` footer for an incompatible change
- `perf:`, `refactor:`, `docs:`, `test:`, `ci:`, `chore:` for the rest (no release)

release-please reads these messages on `main` to build the release PR (version and
`CHANGELOG.md`); a commit without this format is left out of the version and the changelog.
Merging the release PR deploys the version to the VPS (`.github/workflows/release.yml`).

## Checks

CI (`.github/workflows/ci.yml`) must pass to merge a PR, and runs again before a release deploys.
Run the same checks before pushing (the `ci` profile is release without LTO, much faster to build):

```bash
cargo fmt --all --check
cargo clippy --profile ci --workspace --all-targets -- -D warnings
cargo test --profile ci --workspace
cargo run --profile ci -p goldens -- server/goldens
cd client && npm run typecheck && npm run build
```

The golden files are not in git: `tools/goldens_fetch.sh` downloads the set named by
`goldens/ARCHIVE` into `server/goldens`. The goldens run fails on any failure that is not in
`goldens/known_failures.txt`, and on a listed file that passes again (remove it from the list).
Do not add a file to that list to make a run pass: only for a difference that was made on purpose,
with the reason on its line.
