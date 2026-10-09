// Domain entrypoint for the HTTP core. The implementation is in
// `src/core/api/http.ts`; this module exists so callers reach it by name
// (`api/transport`) rather than reaching across layers.
export { API_BASE, request, requestResult } from "../core/api/http";
export type { ApiFailure, ApiResult } from "../core/api/http";
