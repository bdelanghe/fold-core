# SARIF 2.1.0 -> `qualifier record --stdin` lines, one per result with a physical location (qualifier SPEC §5.2, until
# qualifier has its own import-sarif). Usage: jq -c -f tools/sarif-to-qual.jq <file.sarif> | qualifier record --stdin
#
# - kind from the level: error -> fail, warning (SARIF's default) -> concern, note and none -> comment;
# - location from the first physical location, its span start.col:end.col when the region has columns;
# - issuer urn:sarif:<driver>, issuer_type tool; ruleId and any real fingerprint as tags, so a later run can find the
#   record a finding already has.
# A record is a claim, not a gate: nothing here fails a build.
def slug: ascii_downcase | gsub("[^a-z0-9.+-]+"; "-") | ltrimstr("-") | rtrimstr("-");
def uri: sub("^file://"; "") | sub("^\\./"; "");
def span($r):
  if $r.startColumn then "\($r.startLine).\($r.startColumn):\($r.endLine // $r.startLine).\($r.endColumn // $r.startColumn)"
  else "\($r.startLine):\($r.endLine // $r.startLine)" end;
.runs[] | .tool.driver.name as $tool | .results[]?
| (.locations[0].physicalLocation // empty) as $p
| select($p.artifactLocation.uri and $p.region.startLine)
| (.message.text // "" | split("\n")[0]) as $first
| {
    kind: ({"error": "fail", "warning": "concern", "note": "comment", "none": "comment"}[.level // "warning"]),
    location: ($p.artifactLocation.uri | uri),
    span: span($p.region),
    message: (if ($first | length) > 0 then $first else .ruleId end),
    issuer: "urn:sarif:\($tool | slug)",
    issuer_type: "tool",
    tags: (["sarif", "rule:\(.ruleId)"]
      + [(.partialFingerprints // {}), (.fingerprints // {}) | to_entries[] | select(.value != "requires login") | "fingerprint:\(.key)=\(.value)"])
  }
