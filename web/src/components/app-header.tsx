import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import type * as React from "react";

import { api } from "@/api/client";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/i18n";
import { meQuery } from "@/routes/session";

/** Top bar for logged-in pages. */
export function AppHeader({ children }: { children?: React.ReactNode }) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const me = useQuery(meQuery);
  const logout = useMutation({
    mutationFn: () => api.POST("/api/v1/auth/logout"),
    onSuccess: async () => {
      queryClient.clear();
      queryClient.setQueryData(["me"], null);
      await navigate({ to: "/login" });
    },
  });

  return (
    <header className="flex items-center gap-4 border-b px-4 py-2">
      <Link to="/" className="font-semibold">
        {t("common-app-name")}
      </Link>
      <div className="flex min-w-0 flex-1 items-center gap-2">{children}</div>
      {me.data && <span className="text-sm text-muted-foreground">{me.data.name}</span>}
      <Button variant="ghost" size="sm" onClick={() => logout.mutate()} disabled={logout.isPending}>
        {t("auth-logout")}
      </Button>
    </header>
  );
}
