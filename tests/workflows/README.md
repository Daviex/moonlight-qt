# Release workflow checks

Requirements: Python 3.9+, PyYAML, Bash, zip, and GNU find/sort/xargs/sha256sum.
On Windows point `BASH` at Git for Windows' `bin/bash.exe` rather than the WSL launcher.

```sh
python -m pip install PyYAML
python -m unittest discover -s tests/workflows -v
actionlint -shellcheck= -pyflakes=
```

The tests parse the actual reusable release workflow and execute its packaging
and publication scripts. Input files are disposable local fixtures and `gh` is
replaced with a recording fake. No GitHub release or tag is created by these tests.

They verify all platform packages, archive structure, checksums, missing/empty
artifacts, branch-labelled release naming, publication after upload, reruns, and
upload failure leaving a new release in draft state. Actionlint additionally
checks workflow structure, expressions and reusable-workflow inputs.
