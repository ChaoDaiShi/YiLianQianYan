import { requestResult } from "../core/api/http";

export type SecuritySubjectLevel = { subject_id: string } & (
  | { role: "owner"; level: "super"; label: "超级级" }
  | { role: "standard"; level: "user"; label: "用户级" }
  | { role: "restricted"; level: "yilian"; label: "忆涟级" }
);

export function getSecuritySubject() {
  return requestResult<SecuritySubjectLevel>("GET", "/api/security/subject");
}
