/* tsqllint-disable */
ALTER TABLE "subscriptions" ADD COLUMN "retention_days" integer;
--> statement-breakpoint
CREATE INDEX IF NOT EXISTS "download_history_retention_subscription_idx" ON "download_history" ("subscription_id", "status", "finished_at");
