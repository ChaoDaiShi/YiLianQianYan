# UI foundation implementation plan

Approved taskbook baseline: `dea238b65094100ed5c0835a01de7fd7a57a0508`.
Branch: `feat/v1-ui-foundation-responsive`. No merges. No backend, database,
Canvas state/persistence, or v2 changes. Preserve all theme colors and artwork.

1. Capture six actual pages at four desktop sizes before product changes.
2. Establish semantic typography with one default source, then shared controls,
   headers, navigation and shell overlay space. Preserve WorkspaceMode boundaries.
3. Bound Chat reading width and sidebars; update Canvas presentation while keeping
   240 × 128 cards; constrain Settings forms and adapt dashboard grids.
4. Capture the final desktop matrix, computed fonts, click targets, overflow and
   overlay geometry. Check 3440 only for Chat/Canvas/Settings. Test real browser
   zoom separately from Windows DPI (HUMAN_PENDING).
5. Run frontend tests/build/architecture checks once at the final gate, followed
   by real backend core paths and the frozen Canvas stability regression.
6. Record evidence and limitations in the report, commit and push this branch.

Evidence lives under ignored `target/ui-baseline` and `target/ui-foundation`.
Fixtures use real isolated backend APIs; the Chat input is persisted and stopped
through the existing API without claiming any model response or execution success.
Failure evidence includes screenshot, geometry, viewport/DPR, browser and service
logs, and exit status. Never repeatedly rerun a failed matrix without diagnosis.

The existing `--text-secondary` token is a color. Typography therefore uses
`--font-size-*` names to avoid changing the theme color contract.
