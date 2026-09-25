# Behavior cases

Cases are the human-readable source for observable behavior. Use stable IDs within
each document. A test names its case in a nearby comment such as
`Case: docs/cases/repository-checks.md#check-metadata`. Skill cases record requests,
fixtures, and expected actions; their evaluation follows the skill contract.

- [Outline](gist-outline.md): a worked problem-to-case-to-evidence example.
- [Doc review](gist-doc-review.md): trigger scope and review-only behavior.
- [Repository checks](repository-checks.md): content validation and vendor integrity.
- [Packaging](packaging.md): namespaced dependency references.

New and changed behavior uses this convention. Existing tool tests are not yet
fully mapped; the remaining backfill is tracked in `TODO.md`.
