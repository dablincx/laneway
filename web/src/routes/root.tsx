import type { QueryClient } from "@tanstack/react-query";
import { createRootRouteWithContext, Link, Outlet, redirect } from "@tanstack/react-router";

import { fetchInstance } from "@/api/client";
import { useI18n } from "@/i18n";

export type RouterContext = { queryClient: QueryClient };

export const instanceQuery = { queryKey: ["instance"], queryFn: fetchInstance } as const;

export const rootRoute = createRootRouteWithContext<RouterContext>()({
  // Until the first admin exists, everything goes to /setup.
  beforeLoad: async ({ context, location }) => {
    const instance = await context.queryClient.ensureQueryData(instanceQuery);
    const onSetup = location.pathname === "/setup";
    if (instance.needs_setup && !onSetup) throw redirect({ to: "/setup" });
    if (!instance.needs_setup && onSetup) throw redirect({ to: "/login" });
  },
  pendingComponent: Connecting,
  notFoundComponent: NotFound,
  errorComponent: ({ error }) => (
    <Unreachable message={error instanceof Error ? error.message : String(error)} />
  ),
  component: () => (
    <div className="min-h-screen">
      <Outlet />
    </div>
  ),
});

function Connecting() {
  const { t } = useI18n();
  return <p className="p-8 text-sm text-muted-foreground">{t("common-connecting")}</p>;
}

function Unreachable({ message }: { message: string }) {
  const { t } = useI18n();
  return (
    <p role="alert" className="p-8 text-sm text-destructive">
      {t("common-server-unreachable", { error: message })}
    </p>
  );
}

function NotFound() {
  const { t } = useI18n();
  return (
    <main className="mx-auto flex max-w-xl flex-col gap-2 p-8">
      <h1 className="text-xl font-semibold">{t("common-not-found")}</h1>
      <Link to="/" className="text-sm underline underline-offset-4">
        {t("common-back-home")}
      </Link>
    </main>
  );
}
