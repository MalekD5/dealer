# dealer
dealer is the next generation javascript package 'manager' for discreet dependency delivery. No question asked.

## CLI metadata

Run `dealer --help` to see the available commands and `dealer --version` to print the CLI version.

## Commands

### `dealer init`

Creates a `package.json` in the current directory, naming the package after the
directory. An existing manifest is left alone.

### `dealer install [packages...]`

Reads the manifest, resolves every direct and transitive dependency to a single
version, downloads and verifies the tarballs, and links the result into
`node_modules`.

```bash
dealer install
```

Naming packages installs them as well and records them in the manifest:

```bash
dealer install left-pad@^1.3.0 ./fixture-1.0.0.tgz
```

- `--no-save` installs without touching `package.json`.
- `--production` skips `devDependencies`.

Repeated installs are idempotent: packages that are already present at the
right version are left where they are, and packages that have left the manifest
are removed. Anything in `node_modules` that dealer did not create is reported
and left untouched.

### `dealer run <script> [-- args...]`

Runs a script from the manifest's `scripts` table with `node_modules/.bin` on
PATH, so dependency executables can be called by name. Arguments after `--` are
appended to the script, and the script's exit code becomes dealer's.

```bash
dealer run build -- --watch
```

## How packages are stored

Verified tarballs are extracted once into a content-addressed store keyed by
their SHA-512, and projects share that store: installing the same package in a
second project links the files rather than unpacking them again.

| Variable | Purpose | Default |
| --- | --- | --- |
| `DEALER_HOME` | Where the tarball cache and package store live | `%LOCALAPPDATA%\dealer` on Windows, `~/.dealer` elsewhere |
| `DEALER_REGISTRY` | Registry to read metadata and tarballs from | `npm_config_registry`, then `https://registry.npmjs.org` |
