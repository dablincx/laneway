import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createRoute, Link, redirect, useNavigate } from "@tanstack/react-router";

import { boardApi, boardsQuery } from "@/api/boards";
import { errorMessage } from "@/api/errors";
import { AppHeader } from "@/components/app-header";
import { FormError } from "@/components/form";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useI18n } from "@/i18n";
import { instanceQuery, rootRoute } from "./root";
import { meQuery } from "./session";

function Boards() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const boards = useQuery(boardsQuery);
  const instance = useQuery(instanceQuery);
  const create = useMutation({
    mutationFn: (name: string) =>
      boardApi.create(name, [
        { name: t("board-default-column-todo") },
        { name: t("board-default-column-doing") },
        { name: t("board-default-column-done"), is_done: true },
      ]),
    onSuccess: async (created) => {
      await queryClient.invalidateQueries({ queryKey: ["boards"] });
      await navigate({ to: "/boards/$boardId", params: { boardId: created.board.id } });
    },
  });

  return (
    <div className="min-h-screen">
      <AppHeader />
      <main className="mx-auto flex max-w-2xl flex-col gap-6 p-6">
        <h1 className="text-xl font-semibold">{t("board-list-heading")}</h1>
        {boards.data?.length === 0 && (
          <p className="text-sm text-muted-foreground">{t("board-list-empty")}</p>
        )}
        <ul className="flex flex-col gap-2">
          {boards.data?.map((board) => (
            <li key={board.id}>
              <Link
                to="/boards/$boardId"
                params={{ boardId: board.id }}
                className="block rounded-md border px-4 py-3 hover:bg-accent"
              >
                {board.name}
              </Link>
            </li>
          ))}
        </ul>
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            const name = String(new FormData(e.currentTarget).get("name") ?? "");
            create.mutate(name);
          }}
        >
          <Input name="name" aria-label={t("board-new-name")} placeholder={t("board-new-name")} required />
          <Button type="submit" disabled={create.isPending}>
            {t("board-new-submit")}
          </Button>
        </form>
        <FormError message={create.isError ? errorMessage(t, create.error) : null} />
        {instance.data && (
          <p className="text-xs text-muted-foreground">
            {t("common-version", { version: instance.data.version })}
          </p>
        )}
      </main>
    </div>
  );
}

export const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: async ({ context }) => {
    if (!(await context.queryClient.ensureQueryData(meQuery))) throw redirect({ to: "/login" });
  },
  component: Boards,
});
