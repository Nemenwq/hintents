## PR: Refactor RPC configuration parsing into modular validators

Summary
- Split the monolithic parsing/validation/default-assignment for the RPC configuration into distinct lifecycle phases: parse -> defaults -> validate.
- Added a `RPCConfigValidator` interface with two implementations: `UrlsValidator` and `NumericValidator`.
- Updated `RPCConfigParser.loadConfig` to apply defaults and run validators as separate steps.
- Added unit tests covering missing fields, invalid types, and out-of-bounds numeric values.

Files changed
- src/config/rpc-config.ts — refactored parser to separate phases
- src/config/validators.ts — new validator interface and implementations
- src/config/__tests__/rpc-config-validators.spec.ts — new unit tests

Why
- Improves separation of concerns and makes validation logic pluggable and testable.

Notes for reviewers
- The validators live in `src/config/validators.ts` and are intentionally small, focused classes.
- Tests expect clear error messages for invalid/missing inputs.

Attachment (optional)
- Add an image here to show proof (screenshots, logs, etc.):

  [ATTACH IMAGE HERE]

How to attach an image to the PR
1. If you're creating a GitHub Pull Request: after pushing your branch, open the PR page and drag the image into the PR comment box — GitHub will upload it and insert the markdown link.
2. Alternatively, commit the image to the repository (e.g., `docs/`) and reference it in the PR body using the file path.

How to run tests locally
1. Install dependencies:

```bash
npm ci
```

2. Run the test suite:

```bash
npm test
```

Note: In the codespace container I could not run tests because `jest` is not installed in the environment (command `jest: not found`). Running `npm ci` locally will install the dev dependencies and allow the test run.
