import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createRoute, redirect, useNavigate } from "@tanstack/react-router";

import { api, unwrap } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { AuthLayout, Field, FormError } from "@/components/form";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/i18n";
import { rootRoute } from "./root";
import { meQuery } from "./session";

function Login() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const login = useMutation({
    mutationFn: async (form: FormData) =>
      unwrap(
        await api.POST("/api/v1/auth/login", {
          body: { login: String(form.get("login") ?? ""), password: String(form.get("password") ?? "") },
        }),
      ),
    onSuccess: async (me) => {
      queryClient.setQueryData(["me"], me);
      await navigate({ to: "/" });
    },
  });

  return (
    <AuthLayout title={t("auth-login-heading")}>
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          login.mutate(new FormData(e.currentTarget));
        }}
      >
        <Field id="login" label={t("auth-login-field-login")} required autoComplete="username" />
        <Field
          id="password"
          label={t("auth-field-password")}
          type="password"
          required
          autoComplete="current-password"
        />
        <FormError message={login.isError ? errorMessage(t, login.error) : null} />
        <Button type="submit" disabled={login.isPending}>
          {t("auth-login-submit")}
        </Button>
      </form>
    </AuthLayout>
  );
}

export const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/login",
  beforeLoad: async ({ context }) => {
    if (await context.queryClient.ensureQueryData(meQuery)) throw redirect({ to: "/" });
  },
  component: Login,
});
