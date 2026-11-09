import { api, type components, unwrap } from "./client";

export type Board = components["schemas"]["Board"];
export type BoardDetail = components["schemas"]["BoardDetail"];
export type Column = components["schemas"]["Column"];
export type Card = components["schemas"]["Card"];
export type CardChanges = components["schemas"]["UpdateCardRequest"];
export type Priority = components["schemas"]["Priority"];
/** Lowest first. */
export const priorities: Priority[] = ["low", "medium", "high", "urgent"];

export const boardsQuery = {
  queryKey: ["boards"],
  queryFn: async () => unwrap(await api.GET("/api/v1/boards")),
} as const;

export function boardQuery(boardId: string) {
  return {
    queryKey: ["board", boardId],
    queryFn: async () =>
      unwrap(await api.GET("/api/v1/boards/{board_id}", { params: { path: { board_id: boardId } } })),
  } as const;
}

export const boardApi = {
  create: async (name: string, columns: { name: string; is_done?: boolean }[]) =>
    unwrap(await api.POST("/api/v1/boards", { body: { name, columns } })),
  rename: async (boardId: string, name: string) =>
    unwrap(
      await api.PATCH("/api/v1/boards/{board_id}", {
        params: { path: { board_id: boardId } },
        body: { name },
      }),
    ),
  addColumn: async (boardId: string, name: string) =>
    unwrap(
      await api.POST("/api/v1/boards/{board_id}/columns", {
        params: { path: { board_id: boardId } },
        body: { name },
      }),
    ),
  renameColumn: async (columnId: string, name: string) =>
    unwrap(
      await api.PATCH("/api/v1/columns/{column_id}", {
        params: { path: { column_id: columnId } },
        body: { name },
      }),
    ),
  updateColumn: async (
    columnId: string,
    changes: { name?: string; is_done?: boolean; wip_limit?: number | null },
  ) =>
    unwrap(
      await api.PATCH("/api/v1/columns/{column_id}", {
        params: { path: { column_id: columnId } },
        body: changes,
      }),
    ),
  deleteColumn: async (columnId: string) =>
    unwrap(await api.DELETE("/api/v1/columns/{column_id}", { params: { path: { column_id: columnId } } })),
  moveColumn: async (columnId: string, index: number) =>
    unwrap(
      await api.POST("/api/v1/columns/{column_id}/move", {
        params: { path: { column_id: columnId } },
        body: { index },
      }),
    ),
  addCard: async (columnId: string, title: string) =>
    unwrap(
      await api.POST("/api/v1/columns/{column_id}/cards", {
        params: { path: { column_id: columnId } },
        body: { title },
      }),
    ),
  updateCard: async (cardId: string, changes: CardChanges) =>
    unwrap(
      await api.PATCH("/api/v1/cards/{card_id}", {
        params: { path: { card_id: cardId } },
        body: changes,
      }),
    ),
  moveCard: async (cardId: string, columnId: string, index: number) =>
    unwrap(
      await api.POST("/api/v1/cards/{card_id}/move", {
        params: { path: { card_id: cardId } },
        body: { column_id: columnId, index },
      }),
    ),
  deleteCard: async (cardId: string) =>
    unwrap(await api.DELETE("/api/v1/cards/{card_id}", { params: { path: { card_id: cardId } } })),
};
