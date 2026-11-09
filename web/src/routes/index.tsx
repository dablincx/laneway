import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createRoute, redirect, useNavigate } from "@tanstack/react-router";

import { api } from "@/api/client";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/i18n";
import { instanceQuery, rootRoute } from "./root";
import { meQuery } from "./session";

// Placeholder start page until boards exist.
function Home() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const me = useQuery(meQuery);
  const instance = useQuery(instanceQuery);
  const logout = useMutation({
    mutationFn: () => api.POST("/api/v1/auth/logout"),
    onSuccess: async () => {
      queryClient.setQueryData(["me"], null);
      await navigate({ to: "/login" });
    },
  });

  return (
    <main className="mx-auto flex max-w-xl flex-col gap-4 p-8">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">{t("common-app-name")}</h1>
        <Button variant="outline" size="sm" onClick={() => logout.mutate()} disabled={logout.isPending}>
          {t("auth-logout")}
        </Button>
      </div>
      {me.data && <p>{t("home-welcome", { name: me.data.name })}</p>}
      {instance.data && (
        <p className="text-sm text-muted-foreground">
          {t("common-version", { version: instance.data.version })}
        </p>
      )}
    </main>
  );
}

export const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: async ({ context }) => {
    if (!(await context.queryClient.ensureQueryData(meQuery))) throw redirect({ to: "/login" });
  },
  component: Home,
});
