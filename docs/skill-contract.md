# Skill contract

An owned skill is accepted for observable decisions and outcomes, not for its
wording or successful installation. Apply this contract when adding, adopting,
or materially changing one. Existing skills acquire cases as they change.

## Define the behavior

State the outcome, inputs, trigger boundaries, prerequisites, permitted actions,
and stopping conditions. Keep agent-facing guidance in `SKILL.md`; keep acceptance
cases under `docs/cases/<skill-name>.md` so they are not installed as instructions.
Name required tools and skills and what to do if they are missing. Preserve the
user's scope and existing authorization. A review request does not authorize edits.

Every case has a stable ID, a realistic request and fixture, and an observable
expectation. Include a normal request, a nearby request that should not trigger
the skill, and a missing prerequisite or failure. Add cases for consequential
actions. Do not assert exact prose or internal reasoning.

## Hosts and dependencies

Both hosts read `SKILL.md`; the same installed bytes do not imply the same runtime.
Use the named skill through the host's discovery/invocation mechanism. Claude Code
can expose a `Skill` tool and slash commands; Codex supports `$skill-name` and
`/skills`. Do not invent a missing tool.

Default to normal implicit discovery. For a deliberately explicit-only skill,
set `disable-model-invocation: true` in frontmatter and
`policy.allow_implicit_invocation: false` in `agents/openai.yaml`. Keep these policies
consistent. The sidecar can omit presentation fields when none are needed.
See [official Codex skill documentation](https://learn.chatgpt.com/docs/build-skills).

Owned dependencies use their installed `gist-` names. Vendor packaging maps known
package skill names in `/name`, `$name`, and quoted names in paragraphs referring
to the `Skill tool`, including Markdown resources and YAML sidecars. It does not
rewrite ordinary words, resource paths, or unknown host commands such as `/clear`.
New vendor reference conventions require an explicit packaging change and case;
successful copying alone does not certify upstream workflow compatibility.

Use Markdown links for local resources, relative to the containing document.
Keep targets inside the skill directory. `gk check` validates inline local links;
use inline links rather than reference-style links for required skill resources.

## Evaluate and report

Run scenarios in a temporary fixture with the host and tools being claimed.
For trigger cases, expose the normal skill catalog rather than explicitly invoking
the target. For execution cases, invocation may be explicit. Record the host/model,
request, fixture, observed actions/artifact, outcome, and limitations. A manual
run is sufficient; an evaluation service is not required.

Separate static instruction inspection, deterministic tool tests, and actual host
evaluation. Do not label a static walkthrough as an agent run. For two-host support,
record evidence for each host or mark the unevaluated one pending. Preserve the
record in the review handoff; update lasting cases when intended behavior changes.
Have a human or independent context review consequential decisions before merge.

`just ci` checks metadata, resource existence, packaging, and tool behavior. It
cannot establish trigger precision, decision quality, or human approval.
See the [outline worked example](cases/gist-outline.md).
