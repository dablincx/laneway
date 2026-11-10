import { useMutation, useQueryClient } from "@tanstack/react-query";

/** Runs a board change and refreshes the board afterwards. */
export function useBoardMutation<A extends unknown[]>(boardId: string, fn: (...args: A) => Promise<unknown>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (args: A) => fn(...args),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["board", boardId] }),
  });
}
