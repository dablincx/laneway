// API client generated from the server's OpenAPI spec (openapi.json -> schema.ts, run `pnpm gen:api`).
// The web UI only talks to /api/v1 through this.
import createClient from "openapi-fetch";

import type { components, paths } from "./schema";

export type { components };
export type Me = components["schemas"]["Me"];
export type Instance = components["schemas"]["Instance"];

export const api = createClient<paths>({ baseUrl: "/", credentials: "same-origin" });

/** An error response from the API, with its stable code (e.g. `invalid_credentials`). */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
  ) {
    super(code);
  }
}

type Result<T> = { data?: T; error?: unknown; response: Response };

/** Returns the data (undefined for 204 No Content) or throws an ApiError. */
export function unwrap<T>({ data, error, response }: Result<T>): T {
  if (response.ok && error === undefined) return data as T;
  const code = (error as { error?: { code?: string } } | undefined)?.error?.code ?? "unknown";
  throw new ApiError(response.status, code);
}

export async function fetchInstance(): Promise<Instance> {
  return unwrap(await api.GET("/api/v1/instance"));
}

/** The logged-in user, or null when logged out. */
export async function fetchMe(): Promise<Me | null> {
  const result = await api.GET("/api/v1/me");
  if (result.response.status === 401) return null;
  return unwrap(result);
}
