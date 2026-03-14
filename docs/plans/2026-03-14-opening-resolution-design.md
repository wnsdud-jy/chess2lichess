# Opening Resolution Design

**Date:** 2026-03-14

**Problem**

The CLI currently prints opening metadata directly from PGN headers. When a PGN only includes `ECO`, users see low-value output such as `Opening: C40`. Player labels also omit ratings even when `WhiteElo` and `BlackElo` are present.

**Goals**

- Resolve opening names more accurately than raw `ECO` output.
- Prefer an external opening source when available.
- Fall back cleanly when external lookup fails.
- Always print player labels as `Name (rating)`, using `?` when rating is missing.
- Keep conversion behavior best-effort: opening enrichment must never block the main `c2l` flow.

**Non-Goals**

- Building a full local opening explorer.
- Blocking conversion on opening lookup failures.
- Adding new user-facing flags for this iteration.

## Approaches Considered

### 1. Partial local mapping

Hardcode only the most common `ECO -> opening name` pairs in Rust source.

- Pros: simple, no network dependency.
- Cons: weak coverage, quickly falls behind, does not satisfy the request to detect many openings.

### 2. Full local mapping

Bundle a complete `ECO -> opening name` table and use it whenever `Opening` is missing.

- Pros: stable and offline-friendly.
- Cons: still less precise than move-sequence lookup and cannot distinguish multiple lines that share broad ECO groupings.

### 3. External lookup with local fallback

Parse the PGN moves, convert them into an opening-query format, call an external opening source first, and fall back to a local `ECO -> opening name` table when the external lookup fails.

- Pros: best coverage and quality, still resilient offline.
- Cons: adds one network dependency and one local fallback dataset.

**Decision:** approach 3.

## Chosen Design

### Opening resolution flow

1. Parse PGN headers and movetext.
2. Build a move-sequence query from the game moves.
3. Query the Lichess Opening Explorer HTTP API as the primary opening source.
4. If the external response includes an opening object, use its `name` and `eco`.
5. If the external lookup fails, use the PGN `Opening` header if present.
6. If the PGN `Opening` header is missing, use the local `ECO -> opening name` fallback table.
7. If no useful name can be resolved, fall back to raw `ECO`, then `Unknown opening`.

This makes the best available name visible without making the main conversion path fragile.

### Metadata model changes

`GameMetadata` will grow these fields:

- `white_elo`
- `black_elo`

The existing opening-related fields remain, but display helpers will prefer resolved opening data rather than raw PGN header values.

### Player label rules

Player labels always render in this format:

- `WhiteName (WhiteElo) vs BlackName (BlackElo)`

If a rating is missing, render `?`:

- `jyd0323 (?) vs AlpeshRajput08 (1491)`

If only one side exists in metadata, keep the same `Name (rating)` shape for the available side.

### Output behavior

The same resolved metadata rules apply consistently to:

- plain text output
- JSON output
- CSV output
- TUI status panels

This keeps automation and human output aligned.

## Error Handling

Opening lookup is best-effort only. The CLI must still succeed if:

- the external service times out
- the external service returns a non-success status
- the external response lacks opening data
- move parsing fails
- ratings are absent from the PGN

Failures fall through to the local fallback path without interrupting URL conversion, PGN saving, or browser opening behavior.

## Testing Strategy

- Parse `WhiteElo` and `BlackElo` into metadata.
- Render player labels with ratings.
- Render `?` for missing ratings.
- Resolve opening data from an external lookup result.
- Fall back to PGN `Opening` when external lookup fails.
- Fall back to local `ECO` mapping when both external lookup and PGN `Opening` are unavailable.
- Keep JSON and CSV output aligned with the resolved metadata.

## References

- `shakmaty` docs for SAN/UCI parsing: <https://docs.rs/shakmaty>
- Lichess API opening explorer docs: <https://lichess.org/api#tag/Opening-Explorer>
- Lichess opening explorer service repo: <https://github.com/lichess-org/lila-openingexplorer>
- Lichess opening dataset: <https://github.com/lichess-org/chess-openings>
