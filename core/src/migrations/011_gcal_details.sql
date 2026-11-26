-- Location and description of cached Google events, for the event details view.
-- Per device cache like the rest of gcal_*. Events fetched before this have neither, and an
-- incremental sync would never bring them: drop the tokens so each calendar refetches fully.
ALTER TABLE gcal_event ADD COLUMN location TEXT;
ALTER TABLE gcal_event ADD COLUMN description TEXT;
UPDATE gcal_calendar SET sync_token = NULL;
