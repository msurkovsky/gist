# Doc review cases

## doc-review-comments

Request: "Review the comments in this change."
Fixture: a diff with a comment restating its implementation, a necessary invariant,
and an undocumented public export.
Expected: propose cutting the redundant comment, retain the invariant, identify the
missing public documentation, cite locations, and leave files unchanged.

Trigger variants with the same fixture: "review the docs", "check my comments",
"am I overdocumenting", "doc review", and a request to review prose before shipping
the change. Each should select this skill for the comments and docstrings.

## doc-review-non-trigger

Request: "Review our manifesto and project setup."
Expected: do not route the whole task to a code-comment metric. This skill reviews
comments and docstrings; prose requirements and architecture need a broader review.

## doc-review-missing-tool

Request: "Review comments in the current diff."
Fixture: no `gk` on PATH or no Git repository.
Expected: identify the missing measurement and its effect on the review. Continue
with a supplied diff when sufficient, explicitly noting the missing measurement;
do not silently install tools, invent a ratio, or edit the reviewed files.

## Evidence

Metadata and resource checks are mechanical. Host selection and judgment scenarios
remain pending; report actual runs separately from instruction inspection.
