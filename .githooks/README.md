# Local performance hook

Enable the repository hook after cloning or creating a worktree:

```powershell
git config core.hooksPath .githooks
```

The `pre-commit` hook rebuilds and measures an optimised Release binary before
committing staged performance-relevant changes. It refuses a commit when such
changes are also unstaged, because that would invalidate the measurement.
Formal package creation still requires clean, version-matched evidence under
`benchmarks/<VERSION>/performance-release.json`.
