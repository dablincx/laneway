import type { Translate } from "@/i18n";

import { ApiError } from "./client";

// API error code -> message id in locales/*.ftl. Listed one by one so `laneway i18n check`
// sees every id in use.
const messages: Record<string, string> = {
  invalid_credentials: "error-invalid-credentials",
  invalid_setup_token: "error-invalid-setup-token",
  setup_done: "error-setup-done",
  email_taken: "error-email-taken",
  username_taken: "error-username-taken",
  invalid_email: "error-invalid-email",
  invalid_username: "error-invalid-username",
  invalid_name: "error-invalid-name",
  password_too_short: "error-password-too-short",
  password_too_long: "error-password-too-long",
  too_many_attempts: "error-too-many-attempts",
  invalid_board_name: "error-invalid-board-name",
  invalid_column_name: "error-invalid-column-name",
  invalid_card_title: "error-invalid-card-title",
  description_too_long: "error-description-too-long",
  invalid_wip_limit: "error-invalid-wip-limit",
  column_not_empty: "error-column-not-empty",
  not_found: "error-not-found",
};

/** A translated message for any error thrown by an API call. */
export function errorMessage(t: Translate, error: unknown): string {
  const id = error instanceof ApiError ? messages[error.code] : undefined;
  return t(id ?? "error-unknown");
}
