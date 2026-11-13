CREATE TABLE learning_card_game_plans (
    card_id uuid NOT NULL REFERENCES learning_cards(id) ON DELETE CASCADE,
    classifier text NOT NULL,
    game_plan text,
    PRIMARY KEY (card_id, classifier)
);
CREATE INDEX learning_card_game_plan_filter ON learning_card_game_plans(classifier, game_plan, card_id);
