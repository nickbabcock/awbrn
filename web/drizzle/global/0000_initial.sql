CREATE TABLE `account` (
	`id` text PRIMARY KEY NOT NULL,
	`accountId` text NOT NULL,
	`providerId` text NOT NULL,
	`userId` text NOT NULL,
	`accessToken` text,
	`refreshToken` text,
	`idToken` text,
	`accessTokenExpiresAt` integer,
	`refreshTokenExpiresAt` integer,
	`scope` text,
	`password` text,
	`createdAt` integer DEFAULT (unixepoch()) NOT NULL,
	`updatedAt` integer NOT NULL,
	FOREIGN KEY (`userId`) REFERENCES `user`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `account_userId_idx` ON `account` (`userId`);--> statement-breakpoint
CREATE TABLE `map_revisions` (
	`mapId` text NOT NULL,
	`revision` integer NOT NULL,
	`contentHash` text NOT NULL,
	`width` integer NOT NULL,
	`height` integer NOT NULL,
	`playerCount` integer NOT NULL,
	`propertySignature` text NOT NULL,
	`unitSignature` text NOT NULL,
	`rank` text,
	`createdAt` integer DEFAULT (unixepoch()) NOT NULL,
	`lastSeenAt` integer,
	PRIMARY KEY(`mapId`, `revision`),
	FOREIGN KEY (`mapId`) REFERENCES `maps`(`id`) ON UPDATE no action ON DELETE cascade,
	CONSTRAINT "map_revisions_rank_vocabulary" CHECK("map_revisions"."rank" is null or "map_revisions"."rank" in ('C', 'B', 'A', 'S'))
);
--> statement-breakpoint
CREATE UNIQUE INDEX `map_revisions_content_unique` ON `map_revisions` (`mapId`,`contentHash`);--> statement-breakpoint
CREATE INDEX `map_revisions_signature_idx` ON `map_revisions` (`mapId`,`propertySignature`);--> statement-breakpoint
CREATE TABLE `map_sources` (
	`mapId` text PRIMARY KEY NOT NULL,
	`source` text NOT NULL,
	`sourceMapId` integer NOT NULL,
	FOREIGN KEY (`mapId`) REFERENCES `maps`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `map_sources_source_unique` ON `map_sources` (`source`,`sourceMapId`);--> statement-breakpoint
CREATE TABLE `map_tags` (
	`mapId` text NOT NULL,
	`tag` text NOT NULL,
	`addedAt` integer DEFAULT (unixepoch()) NOT NULL,
	PRIMARY KEY(`mapId`, `tag`),
	FOREIGN KEY (`mapId`) REFERENCES `maps`(`id`) ON UPDATE no action ON DELETE cascade,
	CONSTRAINT "map_tags_vocabulary" CHECK("map_tags"."tag" in ('standard', 'fog', 'team', 'ffa', 'high-funds'))
);
--> statement-breakpoint
CREATE INDEX `map_tags_tag_idx` ON `map_tags` (`tag`,`mapId`);--> statement-breakpoint
CREATE TABLE `maps` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`author` text NOT NULL,
	`authorUserId` text,
	`currentRevision` integer NOT NULL,
	`createdAt` integer DEFAULT (unixepoch()) NOT NULL,
	`updatedAt` integer NOT NULL,
	FOREIGN KEY (`authorUserId`) REFERENCES `user`(`id`) ON UPDATE no action ON DELETE set null
);
--> statement-breakpoint
CREATE INDEX `maps_author_idx` ON `maps` (`authorUserId`);--> statement-breakpoint
CREATE TABLE `match_participants` (
	`matchId` text NOT NULL,
	`userId` text NOT NULL,
	`slotIndex` integer NOT NULL,
	`factionId` integer NOT NULL,
	`coId` integer,
	`ready` integer NOT NULL,
	`joinedAt` integer NOT NULL,
	`updatedAt` integer NOT NULL,
	PRIMARY KEY(`matchId`, `slotIndex`),
	FOREIGN KEY (`matchId`) REFERENCES `matches`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`userId`) REFERENCES `user`(`id`) ON UPDATE no action ON DELETE restrict
);
--> statement-breakpoint
CREATE INDEX `match_participants_match_idx` ON `match_participants` (`matchId`);--> statement-breakpoint
CREATE INDEX `match_participants_match_user_idx` ON `match_participants` (`matchId`,`userId`);--> statement-breakpoint
CREATE TABLE `match_results` (
	`matchId` text NOT NULL,
	`slotIndex` integer NOT NULL,
	`userId` text NOT NULL,
	`teamId` text,
	`outcome` text NOT NULL,
	`placement` integer NOT NULL,
	`reason` text,
	`pool` text,
	`recordedAt` integer DEFAULT (unixepoch()) NOT NULL,
	PRIMARY KEY(`matchId`, `slotIndex`),
	FOREIGN KEY (`matchId`) REFERENCES `matches`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`userId`) REFERENCES `user`(`id`) ON UPDATE no action ON DELETE restrict,
	CONSTRAINT "match_results_placement_matches_outcome" CHECK(typeof("match_results"."placement") = 'integer' and "match_results"."placement" >= 1 and ("match_results"."placement" = 1) = ("match_results"."outcome" in ('win', 'draw'))),
	CONSTRAINT "match_results_outcome_vocabulary" CHECK("match_results"."outcome" in ('win', 'loss', 'draw')),
	CONSTRAINT "match_results_reason_null_only_for_standing_win" CHECK("match_results"."reason" is not null or "match_results"."outcome" = 'win')
);
--> statement-breakpoint
CREATE INDEX `match_results_user_idx` ON `match_results` (`userId`,`recordedAt`);--> statement-breakpoint
CREATE INDEX `match_results_pool_idx` ON `match_results` (`pool`,`recordedAt`) WHERE "match_results"."pool" is not null;--> statement-breakpoint
CREATE TABLE `match_voids` (
	`matchId` text PRIMARY KEY NOT NULL,
	`publicReason` text NOT NULL,
	`voidedAt` integer DEFAULT (unixepoch()) NOT NULL,
	FOREIGN KEY (`matchId`) REFERENCES `matches`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `match_voids_voidedAt_idx` ON `match_voids` (`voidedAt`);--> statement-breakpoint
CREATE TABLE `matches` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`phase` text NOT NULL,
	`creatorUserId` text NOT NULL,
	`mapId` text NOT NULL,
	`mapRevision` integer NOT NULL,
	`maxPlayers` integer NOT NULL,
	`isPrivate` integer NOT NULL,
	`joinSlug` text,
	`settings` text NOT NULL,
	`createdAt` integer DEFAULT (unixepoch()) NOT NULL,
	`updatedAt` integer NOT NULL,
	`startedAt` integer,
	`completedAt` integer,
	FOREIGN KEY (`creatorUserId`) REFERENCES `user`(`id`) ON UPDATE no action ON DELETE restrict,
	FOREIGN KEY (`mapId`) REFERENCES `maps`(`id`) ON UPDATE no action ON DELETE restrict,
	FOREIGN KEY (`mapId`,`mapRevision`) REFERENCES `map_revisions`(`mapId`,`revision`) ON UPDATE no action ON DELETE restrict
);
--> statement-breakpoint
CREATE INDEX `matches_creator_idx` ON `matches` (`creatorUserId`);--> statement-breakpoint
CREATE INDEX `matches_browse_idx` ON `matches` (`phase`,`isPrivate`,`createdAt`);--> statement-breakpoint
CREATE UNIQUE INDEX `matches_joinSlug_unique` ON `matches` (`joinSlug`);--> statement-breakpoint
CREATE TABLE `moderation_actions` (
	`id` text PRIMARY KEY NOT NULL,
	`actorUserId` text NOT NULL,
	`action` text NOT NULL,
	`subjectType` text NOT NULL,
	`subjectId` text NOT NULL,
	`reason` text NOT NULL,
	`details` text,
	`createdAt` integer DEFAULT (unixepoch()) NOT NULL,
	FOREIGN KEY (`actorUserId`) REFERENCES `user`(`id`) ON UPDATE no action ON DELETE restrict,
	CONSTRAINT "moderation_actions_action_vocabulary" CHECK("moderation_actions"."action" in ('map.rank', 'map.retag', 'match.void', 'user.ban', 'user.unban', 'user.set-role')),
	CONSTRAINT "moderation_actions_subject_vocabulary" CHECK("moderation_actions"."subjectType" in ('map', 'map_revision', 'match', 'user'))
);
--> statement-breakpoint
CREATE INDEX `moderation_actions_subject_idx` ON `moderation_actions` (`subjectType`,`subjectId`,`createdAt`);--> statement-breakpoint
CREATE INDEX `moderation_actions_actor_idx` ON `moderation_actions` (`actorUserId`,`createdAt`);--> statement-breakpoint
CREATE INDEX `moderation_actions_recent_idx` ON `moderation_actions` (`createdAt`);--> statement-breakpoint
CREATE TABLE `session` (
	`id` text PRIMARY KEY NOT NULL,
	`expiresAt` integer NOT NULL,
	`token` text NOT NULL,
	`createdAt` integer DEFAULT (unixepoch()) NOT NULL,
	`updatedAt` integer NOT NULL,
	`ipAddress` text,
	`userAgent` text,
	`userId` text NOT NULL,
	`impersonatedBy` text,
	FOREIGN KEY (`userId`) REFERENCES `user`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `session_token_unique` ON `session` (`token`);--> statement-breakpoint
CREATE INDEX `session_userId_idx` ON `session` (`userId`);--> statement-breakpoint
CREATE TABLE `user` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`email` text NOT NULL,
	`emailVerified` integer NOT NULL,
	`image` text,
	`role` text,
	`banned` integer DEFAULT false,
	`banReason` text,
	`banExpires` integer,
	`createdAt` integer DEFAULT (unixepoch()) NOT NULL,
	`updatedAt` integer NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `user_email_unique` ON `user` (`email`);--> statement-breakpoint
CREATE TABLE `verification` (
	`id` text PRIMARY KEY NOT NULL,
	`identifier` text NOT NULL,
	`value` text NOT NULL,
	`expiresAt` integer NOT NULL,
	`createdAt` integer DEFAULT (unixepoch()),
	`updatedAt` integer
);
--> statement-breakpoint
CREATE INDEX `verification_identifier_idx` ON `verification` (`identifier`);