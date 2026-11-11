ALTER TABLE app_settings ADD COLUMN bearoff_direction text NOT NULL DEFAULT 'right' CHECK (bearoff_direction IN ('right','left'));
