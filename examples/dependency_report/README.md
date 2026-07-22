# dependency_report

A small CLI that reads this project's own manifest and reports whether the
version dealer installed satisfies the range that was declared.

It exists to exercise a real install: three registry packages, five more pulled
in transitively, a dependency that ships its own executable, and scripts that
reach that executable through `node_modules/.bin`.

## Try it

```bash
dealer install
```

```bash
dealer run start
```

`start` prints the report and exits non-zero if anything is missing or outside
its range.

`sort-versions` calls the `semver` executable that comes with the `semver`
package. Nothing puts it on PATH explicitly — `dealer run` does that:

```bash
dealer run sort-versions -- 2.0.0 1.0.0 10.0.0 1.5.0
```

`satisfies` shows arguments being forwarded to a script that already has flags
of its own:

```bash
dealer run satisfies -- 7.8.5 6.0.0 7.0.1
```

## A note on `^` in scripts

The `satisfies` script quotes its range as `"^7.0.0"` rather than writing it
bare. `^` is cmd's escape character, so on Windows an unquoted `^7.0.0` reaches
the program as `7.0.0` and silently matches nothing. Quoting works in both `sh`
and cmd. This is how any script runner on Windows behaves, dealer included.
