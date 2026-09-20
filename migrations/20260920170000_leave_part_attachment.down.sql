ALTER TABLE timeoff.timeoff_requests DROP CONSTRAINT IF EXISTS leave_part_single_day_only;
ALTER TABLE timeoff.timeoff_requests
    DROP COLUMN IF EXISTS attachment_note,
    DROP COLUMN IF EXISTS attachment_file_id,
    DROP COLUMN IF EXISTS part;
DROP TYPE IF EXISTS leave_part;
