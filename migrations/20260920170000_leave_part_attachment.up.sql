-- Half-day granularity and the certificate on file: a single-day request
-- can be the morning or the afternoon only, and sick leave carries its
-- attachment reference instead of describing one in free text.

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'leave_part') THEN
        CREATE TYPE leave_part AS ENUM ('full', 'am', 'pm');
    END IF;
END
$$;

ALTER TABLE timeoff.timeoff_requests
    ADD COLUMN IF NOT EXISTS part leave_part NOT NULL DEFAULT 'full',
    ADD COLUMN IF NOT EXISTS attachment_file_id uuid,
    ADD COLUMN IF NOT EXISTS attachment_note text;

ALTER TABLE timeoff.timeoff_requests
    DROP CONSTRAINT IF EXISTS leave_part_single_day_only;
ALTER TABLE timeoff.timeoff_requests
    ADD CONSTRAINT leave_part_single_day_only
    CHECK (part = 'full' OR date_start = date_end);
