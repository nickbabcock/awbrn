CREATE TABLE `pending_turns` (
	`matchId` text PRIMARY KEY NOT NULL,
	`matchName` text NOT NULL,
	`deadlineAt` integer,
	`queuedAt` integer DEFAULT (unixepoch()) NOT NULL
);
--> statement-breakpoint
CREATE TABLE `push_subscriptions` (
	`endpoint` text PRIMARY KEY NOT NULL,
	`p256dh` text NOT NULL,
	`auth` text NOT NULL,
	`label` text,
	`createdAt` integer DEFAULT (unixepoch()) NOT NULL,
	`failureCount` integer DEFAULT 0 NOT NULL
);
