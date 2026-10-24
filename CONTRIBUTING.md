# Contributing

The repository follows the Aidoku Community layout: each source lives under
`sources/<language>.<name>/`, with its Rust manifest, `src/`, and `res/` resources.
Shared build scripts, tests, the vendored SDK and licenses live at the root.

Run the build and validation commands in README.md before opening a pull request.
Use conventional commit messages such as `fix(multi.pixiv): renew expired tokens`.
Increment `info.version` in `sources/multi.pixiv/res/source.json` when shipping a
source update. Never commit real Pixiv credentials, cookies, tokens or account
backups. Tests use synthetic credentials and mocked HTTP responses.

The `main` branch contains source code and a ready-to-import package in `dist/`.
After all checks pass, GitHub Actions publishes the standard source-list index,
versioned packages and icons to `gh-pages`. Pull requests only run checks and
produce downloadable build artifacts.
