# Closed-source release gate

The repository currently contains an `AGPL-3.0` license file. A binary-only
distribution must not be published until the copyright owner has supplied and
approved a separate commercial/distribution license for this product and the
third-party dependency notices have been reviewed.

The packaging scripts therefore require:

1. `release/COMMERCIAL_LICENSE.txt`, approved for the intended recipients;
2. a clean committed source revision;
3. target-native build and test gates.

Stripping symbols, LTO, or omitting source files does not replace this license
approval. This file is an engineering release gate, not legal advice.
