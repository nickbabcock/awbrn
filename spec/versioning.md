# Ruleset and specification versioning

`spec_version` versions the AWVM document and interchange formats.
`ruleset.id` identifies a game dialect and `ruleset.revision` identifies the
observed AWBW behavior being specified.

Initial identifiers:

```json
{
  "spec_version": "0.1.0",
  "ruleset": { "id": "awbw", "revision": "2026-07-10" }
}
```

The date is an AWVM behavior revision label; it does not claim that AWBW was
deployed on that date. Once published, a revision is immutable.

The specification uses semantic versioning:

- patch: clarification or additional fixture with no changed valid result;
- minor: backward-compatible syntax or newly specified feature;
- major: incompatible schema or semantic change.

A discovered AWBW behavior difference creates a new ruleset revision. Existing
fixtures remain associated with their original revision. Implementations MUST
reject unknown ruleset identifiers or revisions rather than silently applying
another ruleset.
