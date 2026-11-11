ALTER TABLE app_settings
 ADD COLUMN default_play_mode text NOT NULL DEFAULT 'coach' CHECK (default_play_mode IN ('competition','training','coach','tutor')),
 ADD COLUMN tutor_threshold double precision NOT NULL DEFAULT 0.04 CHECK (tutor_threshold > 0 AND tutor_threshold <= 1),
 ADD COLUMN training_auto_analysis boolean NOT NULL DEFAULT true;

-- Keep the learning context with copies and detached library entries.
ALTER TABLE matches ADD COLUMN play_mode text,
 ADD COLUMN play_assisted boolean NOT NULL DEFAULT false;
UPDATE matches m SET play_mode=coalesce(s.state->>'play_mode','competition')
 FROM play_sessions s WHERE s.library_match_id=m.id;
