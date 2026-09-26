# Stable Studio C2 behavior contract

1. State/execution refresh updates node data only; existing local positions and
   dimensions remain authoritative for the mounted canvas.
2. Selecting via canvas, keyboard or Trail changes selection only. It does not
   locate, fit, pan, open Inspector, or trigger automatic layout.
3. Delayed saves and stale server views cannot roll a local drag back. The existing
   write queue serializes updates and rebases once on stale_view_revision; blocked
   writes retain local changes and expose retry.
4. Auto Layout, Locate, Fit and 100% run only on explicit user actions. Plus/minus
   are also explicit. Panel visibility, status updates and window resize do not
   issue camera commands.
5. Re-entering a graph restores its saved viewport. Collapsing auxiliary panels
   does not recreate ReactFlow or discard Inspector drafts.
6. Every card stays 240x128 before/during/after execution. Existing node IDs,
   saved layouts, graph-order Trail and group semantics are unchanged.
7. Save labels reflect actual queue notifications: saving, synced revision, or
   failed with retry. No timer-generated or optimistic "saved" completion.
8. Run selected node follows existing configured executor and graph lock rules.
   Backend authorization and approval remain authoritative. No bypass or separate
   execution path is introduced by the toolbar.
9. Publishing/MCP UI slots are disabled and explicitly unconnected. Existing
   executor references are displayed as data; no new protocol is implied.
10. Header/actions, floating tools and visible auxiliary panel must fit desktop
    viewports. The global voice reserved area remains respected. Dark/light
    themes and keyboard focus remain readable.

Evidence: existing queue/reconcile/camera tests, unchanged stability assertions
(only explicit panel opening added), plus real backend + system Edge Studio
screenshots, geometry, toolbar/panel tests and final result JSON in ignored target.
