import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createRoute, useNavigate } from "@tanstack/react-router";

import { api, type Instance, unwrap } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { AuthLayout, Field, FormError } from "@/components/form";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/i18n";
import { rootRoute } from "./root";

function Setup() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const setup = useMutation({
    mutationFn: async (form: FormData) =>
      unwrap(
        await api.POST("/api/v1/setup", {
          body: {
            setup_token: String(form.get("setup_token") ?? ""),
            email: String(form.get("email") ?? ""),
            username: String(form.get("username") ?? ""),
            name: String(form.get("name") ?? ""),
            password: String(form.get("password") ?? ""),
          },
        }),
      ),
    onSuccess: async (me) => {
      queryClient.setQueryData(["me"], me);
      // The cached instance still says "needs setup", which would redirect back here.
      queryClient.setQueryData<Instance>(["instance"], (old) => old && { ...old, needs_setup: false });
      await navigate({ to: "/" });
    },
  });

  return (
    <AuthLayout title={t("setup-heading")} intro={t("setup-intro")}>
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          setup.mutate(new FormData(e.currentTarget));
        }}
      >
        <Field id="setup_token" label={t("setup-field-token")} required autoComplete="off" />
        <Field id="name" label={t("auth-field-name")} required autoComplete="name" />
        <Field id="email" label={t("auth-field-email")} type="email" required autoComplete="email" />
        <Field id="username" label={t("auth-field-username")} required autoComplete="username" />
        <Field
          id="password"
          label={t("auth-field-password")}
          type="password"
          required
          minLength={10}
          autoComplete="new-password"
        />
        <FormError message={setup.isError ? errorMessage(t, setup.error) : null} />
        <Button type="submit" disabled={setup.isPending}>
          {t("setup-submit")}
        </Button>
      </form>
    </AuthLayout>
  );
}

export const setupRoute = createRoute({ getParentRoute: () => rootRoute, path: "/setup", component: Setup });
