// Temporary hand-written client. Gets replaced by the client generated from the
// OpenAPI spec once the server publishes one (see TODO.md).

export type Instance = { name: string; version: string };

async function get<T>(path: string): Promise<T> {
  const response = await fetch(`/api/v1${path}`, { headers: { Accept: "application/json" } });
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  return (await response.json()) as T;
}

export const api = {
  instance: () => get<Instance>("/instance"),
};
