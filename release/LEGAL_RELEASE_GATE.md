# Internal laboratory release policy

This release profile is intended for non-commercial training and research
inside the owning laboratory. Packaging does not require a commercial license
file. Every package carries the repository `LICENSE`, the internal-use notice,
the exact source commit, and a SHA256 manifest.

The engineering gates are:

1. a clean committed source revision;
2. target-native simulator and SDK builds;
3. all SDK tests and compatibility checks pass;
4. no simulator implementation source, debug symbols, inference models, or
   editable simulator configuration is placed in the package.

Do not republish this internal package or provide it to another organization
without a separate licensing and dependency review. This file documents the
release profile; it is not legal advice.
