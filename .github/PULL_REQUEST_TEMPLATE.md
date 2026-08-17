## What changed?

<!-- Summary of the change. -->

## Why?

<!-- The motivation — link an issue if there is one. -->

## Tests?

<!-- What did you run? `cargo fmt --all -- --check`, `cargo clippy ... -D warnings`,
     `cargo test --workspace --locked --no-fail-fast` are expected to pass —
     see CONTRIBUTING.md. -->

## Scientific impact?

<!-- Does this change what a score, dataset, or metric means? If yes, see
     CONTRIBUTING.md's "Changes to models, labels, datasets, features,
     scoring, sampling, or calibration" section. -->

## Dataset impact?

<!-- New dataset, changed dataset, changed feature/label definition? -->

## Model impact?

<!-- New model, changed model, changed training procedure? -->

## Production impact?

<!-- Does this touch the serving path, migrations, or deployment config?
     Migrations are additive-only — see docs/deployment.md. -->

## Breaking change?

<!-- API, config, or CLI surface change that existing deployments would need
     to react to. -->

## Security impact?

<!-- New endpoint, new dependency, new external call, new secret/credential
     handling? -->

---

- [ ] This PR does not activate or promote an experimental model unless
      explicitly approved (see `GOVERNANCE.md`).
