/** The vision default is only changed after the requested chat model is verified and activated. */
export async function finishQuickModelSetup(actions: {
  verify: () => Promise<unknown>;
  activate: () => Promise<unknown>;
  configureVision: () => Promise<void>;
}): Promise<"verify_failed" | "activate_failed" | "ready"> {
  if (!await actions.verify()) return "verify_failed";
  if (!await actions.activate()) return "activate_failed";
  await actions.configureVision();
  return "ready";
}
