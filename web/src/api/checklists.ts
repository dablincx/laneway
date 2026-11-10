import { api, type components, unwrap } from "./client";

export type Checklist = components["schemas"]["Checklist"];
export type ChecklistItem = components["schemas"]["ChecklistItem"];

export const checklistApi = {
  create: async (cardId: string, title: string) =>
    unwrap(
      await api.POST("/api/v1/cards/{card_id}/checklists", {
        params: { path: { card_id: cardId } },
        body: { title },
      }),
    ),
  rename: async (checklistId: string, title: string) =>
    unwrap(
      await api.PATCH("/api/v1/checklists/{checklist_id}", {
        params: { path: { checklist_id: checklistId } },
        body: { title },
      }),
    ),
  delete: async (checklistId: string) =>
    unwrap(
      await api.DELETE("/api/v1/checklists/{checklist_id}", {
        params: { path: { checklist_id: checklistId } },
      }),
    ),
  addItem: async (checklistId: string, text: string) =>
    unwrap(
      await api.POST("/api/v1/checklists/{checklist_id}/items", {
        params: { path: { checklist_id: checklistId } },
        body: { text },
      }),
    ),
  updateItem: async (itemId: string, changes: { text?: string; done?: boolean }) =>
    unwrap(
      await api.PATCH("/api/v1/checklist-items/{item_id}", {
        params: { path: { item_id: itemId } },
        body: changes,
      }),
    ),
  deleteItem: async (itemId: string) =>
    unwrap(await api.DELETE("/api/v1/checklist-items/{item_id}", { params: { path: { item_id: itemId } } })),
};
