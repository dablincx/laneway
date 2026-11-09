import { MutationCache, QueryCache, QueryClient } from "@tanstack/react-query";
import { createRouter } from "@tanstack/react-router";
import { ApiError } from "./api/client";
import { boardRoute } from "./routes/board";
import { indexRoute } from "./routes/index";
import { loginRoute } from "./routes/login";
import { rootRoute } from "./routes/root";
import { setupRoute } from "./routes/setup";

/** The session ended (expired, logged out elsewhere): forget the user and go to the login page. */
function onApiError(error: unknown) {
  if (error instanceof ApiError && error.code === "unauthenticated") {
    queryClient.setQueryData(["me"], null);
    void router.navigate({ to: "/login" });
  }
}

export const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
  queryCache: new QueryCache({ onError: onApiError }),
  mutationCache: new MutationCache({ onError: onApiError }),
});

const routeTree = rootRoute.addChildren([indexRoute, boardRoute, loginRoute, setupRoute]);

export const router = createRouter({ routeTree, context: { queryClient } });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
