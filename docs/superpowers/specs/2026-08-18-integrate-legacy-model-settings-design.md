# Integrate Legacy Model Settings Design

## Goal

Make the new model management panel the single visual entry point for model configuration. The existing legacy runtime settings must remain available and functional, but must be rendered inside the model management panel instead of as a second section below it.

## Design

`SettingsPage` continues to own the persisted `AppConfig` state and existing save/clear-secret handlers. It passes the legacy model configuration and callbacks to `ModelManagerPanel`. `ModelManagerPanel` renders the model archive management, usage chart, and the legacy runtime/embedding fields as one cohesive model module. The old standalone JSX is removed from `SettingsPage`.

No backend API, database schema, or configuration shape changes are needed. This is a presentation/composition change, so existing persistence and secret redaction behavior remain unchanged.

## Behavior

- The model settings section contains one model management module.
- Existing legacy fields remain editable and use the existing `SettingsPage` save flow.
- Existing API-key clear actions and migration warnings remain available inside the module.
- The settings page must not render a second standalone legacy model section after `ModelManagerPanel`.

## Verification

- Add a frontend contract test that asserts the settings page passes legacy model controls to the model manager and no longer owns standalone legacy model labels/inputs.
- Run the focused frontend test, the complete frontend test suite, and the production build.
- Run `cargo fmt --check` and `cargo test` to verify no backend regression.
