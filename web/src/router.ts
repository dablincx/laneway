import { QueryClient } from "@tanstack/react-query";
import { createRouter } from "@tanstack/react-router";

import { indexRoute } from "./routes/index";
import { loginRoute } from "./routes/login";
import { rootRoute } from "./routes/root";
import { setupRoute } from "./routes/setup";

export const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });

const routeTree = rootRoute.addChildren([indexRoute, loginRoute, setupRoute]);

export const router = createRouter({ routeTree, context: { queryClient } });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
