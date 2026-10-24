export function desktopGate(observation) {
  return observation.windows.length === 1
    && observation.windows[0].document_matches_expected === true
    && observation.windows[0].visible === true
    && observation.windows[0].focused_document === true;
}
