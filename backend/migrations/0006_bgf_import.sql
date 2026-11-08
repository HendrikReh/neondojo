-- BGBlitz BGF joins the source formats stored by the common import pipeline.
ALTER TABLE matches DROP CONSTRAINT matches_format_check;
ALTER TABLE matches ADD CONSTRAINT matches_format_check
    CHECK (format IN ('mat', 'sgf', 'xg', 'bgf'));
