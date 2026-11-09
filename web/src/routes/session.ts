import { fetchMe } from "@/api/client";

export const meQuery = { queryKey: ["me"], queryFn: fetchMe } as const;
