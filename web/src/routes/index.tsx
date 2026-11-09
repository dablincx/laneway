import { useQuery } from "@tanstack/react-query";
import { createRoute } from "@tanstack/react-router";

import { useI18n } from "@/i18n";
import { api } from "@/lib/api";
import { rootRoute } from "./root";

// Placeholder start page until boards exist.
function Home() {
  const { t } = useI18n();
  const instance = useQuery({ queryKey: ["instance"], queryFn: api.instance });
  return (
    <main className="mx-auto flex max-w-xl flex-col gap-2 p-8">
      <h1 className="text-2xl font-semibold">{t("common-app-name")}</h1>
      <p className="text-sm text-muted-foreground">
        {instance.isPending && t("common-connecting")}
        {instance.isError && t("common-server-unreachable", { error: instance.error.message })}
        {instance.data && t("common-version", { version: instance.data.version })}
      </p>
    </main>
  );
}

export const indexRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", component: Home });
