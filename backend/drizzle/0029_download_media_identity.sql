ALTER TABLE "downloads" ADD COLUMN "source_video_id" text;
--> statement-breakpoint
ALTER TABLE "downloads" ADD COLUMN "platform" text;
--> statement-breakpoint
ALTER TABLE "downloads" ADD COLUMN "media_type" text DEFAULT 'video';
--> statement-breakpoint
UPDATE "downloads" SET "media_type" = 'video' WHERE "media_type" IS NULL;
