import { createContext, useEffect, useState, type ReactNode } from "react";
import { managementRequest } from "../../api/productManagement";

export const ProcessDisplay = createContext("");

export function ProcessDisplayProvider({ children }: { children: ReactNode }) {
  const [preference, setPreference] = useState("");
  useEffect(() => {
    const controller = new AbortController();
    void managementRequest<{profile: {fields: {show_steps?: string}}}>("GET", "/api/v1/profile", undefined, controller.signal)
      .then(value => { if (!controller.signal.aborted) setPreference(value.profile.fields.show_steps ?? ""); })
      .catch(() => undefined);
    return () => controller.abort();
  }, []);
  return <ProcessDisplay.Provider value={preference}>{children}</ProcessDisplay.Provider>;
}
