//! Study overview, reports, AI match assessments, recurring mistakes,
//! FSRS flashcards, decks, topics, practice rounds, transfer practice,
//! Anki local-disk transfers/sync, and in-process `WKURLSchemeHandler`
//! Anki card face & media lookups.

use super::BackendFacade;
use crate::{
    configuration::ConfigurationLocale,
    domain_error::DomainError,
    learning::{
        self, AddCards, AddCardsResult, AddDeckCards, AddDeckCardsResult, AnkiConnectStatus,
        AnkiMediaBlob, AnkiSyncHistoryEntry, AttemptedCard, AutoSyncSettings, CardAnswer,
        CardAttemptRequest, CardBrowseItem, CardPage, CardQuery, CardSuggestions, CardTags,
        CardTopic, CreateDeck, Deck, DeleteCards, DuplicatePage, DuplicateQuery, EditCardTags,
        ExportQuery, ImportedAnkiDeck, LearningPreferences, LearningSummary, LinkedAnkiDeck,
        MoveCards, MoveCardsResult, NextCard, PipAnswer, PipResult, PracticeRound, RemoveCards,
        RenameDeck, ResolveDuplicate, RevealedCard, ReviewCard, ReviewResult, RoundAdvance,
        RoundAnswer, SaveCardTopic, SaveSuggestions, ScoreAnswer, ScorePractice, SuggestionQuery,
        SyncAction, SyncActionRequest, SyncAttemptResult, SyncResult, TransferRequest,
        TransferSummary,
    },
    match_assessment::{self, AssessmentState},
    model::MatchSummary,
    play::{PlayHistorySnapshots, PlaySession},
    practice::evaluation::PracticeEvaluation,
    recurring_mistakes::{
        self, MistakeAnswer, MistakeAttempt, MistakeAttemptRequest, MistakePractice,
        MistakePracticeHistory, MistakePracticeRequest, MistakeReference, Mistakes, MistakesQuery,
    },
    study::{self, ReportQuery, StudyReportPage},
};
use std::path::{Path, PathBuf};
use uuid::Uuid;

impl BackendFacade {
    // ── Study Overview, Reports, Assessments & Recurring Mistakes ────────────

    pub async fn study_overview(&self) -> Result<LearningSummary, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::learning_summary_service(&self.state).await
    }

    pub async fn list_study_reports(
        &self,
        query: ReportQuery,
    ) -> Result<StudyReportPage, DomainError> {
        let _guard = self.db_access_guard()?;
        study::list_reports_service(&self.state, query).await
    }

    pub async fn get_study_report(&self, report_id: Uuid) -> Result<PlaySession, DomainError> {
        let _guard = self.db_access_guard()?;
        study::get_report_service(&self.state, report_id).await
    }

    pub async fn study_report_history(
        &self,
        report_id: Uuid,
        version: i64,
    ) -> Result<PlayHistorySnapshots, DomainError> {
        let _guard = self.db_access_guard()?;
        study::report_history_service(&self.state, report_id, version).await
    }

    pub async fn get_match_assessment(
        &self,
        report_id: Uuid,
        language: ConfigurationLocale,
    ) -> Result<AssessmentState, DomainError> {
        let _guard = self.db_access_guard()?;
        match_assessment::get_assessment_service(&self.state, report_id, language).await
    }

    pub async fn start_match_assessment(
        &self,
        report_id: Uuid,
        language: ConfigurationLocale,
        report_version: i64,
    ) -> Result<AssessmentState, DomainError> {
        let _guard = self.db_access_guard()?;
        match_assessment::start_assessment_service(&self.state, report_id, language, report_version)
            .await
    }

    pub async fn list_recurring_mistakes(
        &self,
        query: MistakesQuery,
    ) -> Result<Mistakes, DomainError> {
        let _guard = self.db_access_guard()?;
        recurring_mistakes::list_mistakes_service(&self.state, query).await
    }

    pub async fn practice_recurring_mistake(
        &self,
        request: MistakePracticeRequest,
    ) -> Result<MistakePractice, DomainError> {
        let _guard = self.db_access_guard()?;
        recurring_mistakes::practice_mistakes_service(&self.state, request).await
    }

    pub async fn reveal_recurring_mistake(
        &self,
        reference: &MistakeReference,
    ) -> Result<MistakeAnswer, DomainError> {
        let _guard = self.db_access_guard()?;
        recurring_mistakes::reveal_mistake_service(&self.state, reference).await
    }

    pub async fn attempt_recurring_mistake(
        &self,
        request: MistakeAttemptRequest,
    ) -> Result<MistakeAttempt, DomainError> {
        let _guard = self.db_access_guard()?;
        recurring_mistakes::attempt_mistake_service(&self.state, request).await
    }

    pub async fn list_mistake_practice_history(
        &self,
    ) -> Result<MistakePracticeHistory, DomainError> {
        let _guard = self.db_access_guard()?;
        recurring_mistakes::list_history_service(&self.state).await
    }

    pub async fn list_unanalyzed_matches(
        &self,
        player: &str,
        database_id: Option<Uuid>,
        independent_only: bool,
        limit: Option<i64>,
    ) -> Result<Vec<MatchSummary>, DomainError> {
        let _guard = self.db_access_guard()?;
        recurring_mistakes::list_unanalyzed_matches_service(
            &self.state,
            player,
            database_id,
            independent_only,
            limit,
        )
        .await
    }

    // ── Learning Preferences, Decks, Cards, Topics & Duplicates ──────────────

    pub async fn get_learning_preferences(&self) -> Result<LearningPreferences, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::read_preferences(&self.state).await
    }

    pub async fn update_learning_preferences(
        &self,
        preferences: LearningPreferences,
    ) -> Result<LearningPreferences, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::write_preferences(&self.state, preferences).await
    }

    pub async fn list_decks(&self) -> Result<Vec<Deck>, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::list_decks_service(&self.state).await
    }

    pub async fn create_deck(&self, name: String) -> Result<Deck, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::create_deck_service(&self.state, CreateDeck { name }).await
    }

    pub async fn update_deck(
        &self,
        deck_id: Uuid,
        name: String,
        expected_version: i64,
    ) -> Result<Deck, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::rename_deck_service(
            &self.state,
            deck_id,
            RenameDeck {
                name,
                expected_version,
            },
        )
        .await
    }

    pub async fn delete_deck(
        &self,
        deck_id: Uuid,
        expected_version: i64,
    ) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        learning::delete_deck_service(&self.state, deck_id, expected_version).await
    }

    pub async fn search_cards(
        &self,
        deck_id: Option<Uuid>,
        query: CardQuery,
    ) -> Result<CardPage, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::search_cards_service(&self.state, deck_id, query).await
    }

    pub async fn get_card(&self, card_id: Uuid) -> Result<CardBrowseItem, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::get_card_service(&self.state, card_id).await
    }

    pub async fn preview_card(&self, card_id: Uuid) -> Result<CardAnswer, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::preview_card_service(&self.state, card_id).await
    }

    pub async fn add_cards(
        &self,
        deck_id: Uuid,
        request: AddCards,
    ) -> Result<AddCardsResult, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::add_cards_to_deck_service(&self.state, deck_id, request).await
    }

    pub async fn suggest_cards(
        &self,
        deck_id: Uuid,
        query: SuggestionQuery,
    ) -> Result<CardSuggestions, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::preview_suggestions_service(&self.state, deck_id, query).await
    }

    pub async fn save_suggested_cards(
        &self,
        deck_id: Uuid,
        request: SaveSuggestions,
    ) -> Result<AddCardsResult, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::save_suggestions_service(&self.state, deck_id, request).await
    }

    pub async fn add_deck_cards(
        &self,
        deck_id: Uuid,
        request: AddDeckCards,
    ) -> Result<AddDeckCardsResult, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::add_deck_cards_service(&self.state, deck_id, request).await
    }

    pub async fn move_cards(
        &self,
        source_deck_id: Uuid,
        request: MoveCards,
    ) -> Result<MoveCardsResult, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::move_cards_service(&self.state, source_deck_id, request).await
    }

    pub async fn remove_deck_cards(
        &self,
        deck_id: Uuid,
        request: RemoveCards,
    ) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        learning::remove_cards_service(&self.state, deck_id, request).await
    }

    pub async fn delete_cards(&self, request: DeleteCards) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        learning::delete_cards_service(&self.state, request).await
    }

    pub async fn suggest_card_tags(&self) -> Result<Vec<String>, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::suggest_tags_service(&self.state).await
    }

    pub async fn update_card_tags(
        &self,
        request: EditCardTags,
    ) -> Result<Vec<CardTags>, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::edit_tags_service(&self.state, request).await
    }

    pub async fn list_card_topics(&self) -> Result<Vec<CardTopic>, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::list_topics_service(&self.state).await
    }

    pub async fn save_card_topic(&self, request: SaveCardTopic) -> Result<CardTopic, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::save_topic_service(&self.state, request).await
    }

    pub async fn delete_card_topic(&self, topic_id: Uuid) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        learning::delete_topic_service(&self.state, topic_id).await
    }

    pub async fn list_duplicate_cards(
        &self,
        query: DuplicateQuery,
    ) -> Result<DuplicatePage, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::list_duplicates_service(&self.state, query).await
    }

    pub async fn merge_duplicate_cards(
        &self,
        request: ResolveDuplicate,
    ) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        learning::resolve_duplicate_service(&self.state, request).await
    }

    // ── FSRS Queue, Practice Rounds, Exercises & Transfer Practice ───────────

    pub async fn next_fsrs_card(&self, deck_id: Uuid) -> Result<NextCard, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::next_card_service(&self.state, deck_id).await
    }

    pub async fn reveal_fsrs_card(
        &self,
        card_id: Uuid,
        expected_version: i64,
    ) -> Result<RevealedCard, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::reveal_card_service(&self.state, card_id, expected_version).await
    }

    pub async fn submit_fsrs_attempt(
        &self,
        card_id: Uuid,
        request: CardAttemptRequest,
    ) -> Result<AttemptedCard, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::submit_card_attempt_service(&self.state, card_id, request).await
    }

    pub async fn submit_fsrs_grade(
        &self,
        card_id: Uuid,
        request: ReviewCard,
    ) -> Result<ReviewResult, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::review_card_service(&self.state, card_id, request).await
    }

    pub async fn start_practice_round(
        &self,
        query: CardQuery,
    ) -> Result<PracticeRound, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::start_round_service(&self.state, query).await
    }

    pub async fn get_practice_round(&self, round_id: Uuid) -> Result<PracticeRound, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::read_round_service(&self.state, round_id).await
    }

    pub async fn submit_practice_round_item(
        &self,
        round_id: Uuid,
        answer: RoundAnswer,
    ) -> Result<PracticeRound, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::answer_round_service(&self.state, round_id, answer).await
    }

    pub async fn advance_practice_round(
        &self,
        round_id: Uuid,
        advance: RoundAdvance,
    ) -> Result<PracticeRound, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::advance_round_service(&self.state, round_id, advance).await
    }

    pub async fn mark_practice_round(
        &self,
        round_id: Uuid,
        mark: RoundAdvance,
    ) -> Result<PracticeRound, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::mark_round_service(&self.state, round_id, mark).await
    }

    pub async fn retry_practice_round(
        &self,
        round_id: Uuid,
        version: i64,
    ) -> Result<PracticeRound, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::retry_round_service(&self.state, round_id, version).await
    }

    pub async fn check_card_pips(&self, answer: PipAnswer) -> Result<PipResult, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::check_pips_service(&self.state, answer).await
    }

    pub async fn start_score_variants(&self, card_id: Uuid) -> Result<ScorePractice, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::score_questions_service(&self.state, card_id).await
    }

    pub async fn score_practice_variants(
        &self,
        card_id: Uuid,
        answer: ScoreAnswer,
    ) -> Result<PracticeEvaluation, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::score_answer_service(&self.state, card_id, answer).await
    }

    pub async fn transfer_practice_history(&self) -> Result<Vec<TransferSummary>, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::transfer_history_service(&self.state).await
    }

    pub async fn start_transfer_practice(
        &self,
        request: TransferRequest,
    ) -> Result<PracticeRound, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::start_transfer_service(&self.state, request).await
    }

    // ── Anki Local-Disk Transfers, Sync & WKURLSchemeHandler Lookups ─────────

    pub async fn export_deck_apkg_to_path(
        &self,
        deck_id: Uuid,
        locale: Option<ConfigurationLocale>,
        destination: &Path,
    ) -> Result<PathBuf, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::export_deck_apkg_to_path_service(&self.state, deck_id, locale, destination).await
    }

    pub async fn import_deck_apkg_from_path(
        &self,
        deck_id: Uuid,
        path: &Path,
    ) -> Result<AddCardsResult, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::import_deck_apkg_from_path(&self.state, deck_id, path).await
    }

    pub async fn import_anki_package_from_path(
        &self,
        path: &Path,
    ) -> Result<Vec<ImportedAnkiDeck>, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::import_anki_package_from_path(&self.state, path).await
    }

    /// Probes the local AnkiConnect instance without touching the database.
    pub async fn anki_connect_status(&self) -> AnkiConnectStatus {
        learning::probe_connect_status().await
    }

    pub async fn send_deck_to_anki(
        &self,
        deck_id: Uuid,
        locale: Option<ConfigurationLocale>,
    ) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        learning::send_deck_to_anki_service(&self.state, deck_id, ExportQuery { locale }).await
    }

    pub async fn sync_deck_with_anki(&self, deck_id: Uuid) -> Result<SyncResult, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::sync_deck_with_anki_service(&self.state, deck_id).await
    }

    pub async fn anki_sync_overview(&self) -> Result<Vec<LinkedAnkiDeck>, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::anki_sync_overview_service(&self.state).await
    }

    pub async fn anki_sync_history(
        &self,
        deck_id: Uuid,
    ) -> Result<Vec<AnkiSyncHistoryEntry>, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::anki_sync_history_service(&self.state, deck_id).await
    }

    pub async fn get_anki_auto_sync(&self, deck_id: Uuid) -> Result<AutoSyncSettings, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::get_anki_auto_sync_service(&self.state, deck_id).await
    }

    pub async fn update_anki_auto_sync(
        &self,
        deck_id: Uuid,
        enabled: bool,
    ) -> Result<AutoSyncSettings, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::set_anki_auto_sync_service(&self.state, deck_id, enabled).await
    }

    pub async fn anki_auto_sync_action(
        &self,
        deck_id: Uuid,
        request: SyncActionRequest,
    ) -> Result<Option<SyncAttemptResult>, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::anki_auto_sync_act_service(&self.state, deck_id, request).await
    }

    pub async fn retry_anki_sync_error(
        &self,
        deck_id: Uuid,
        retry_of: Option<Uuid>,
        include_result: bool,
    ) -> Result<Option<SyncAttemptResult>, DomainError> {
        let _guard = self.db_access_guard()?;
        let request = match retry_of {
            Some(entry_id) => SyncActionRequest {
                action: SyncAction::SyncNow,
                retry_of: Some(entry_id),
                include_result: false,
            },
            None => SyncActionRequest {
                action: SyncAction::RetryOpenErrors,
                retry_of: None,
                include_result,
            },
        };
        learning::anki_auto_sync_act_service(&self.state, deck_id, request).await
    }

    /// Returns the rendered HTML document for an imported Anki card face (`"front"` or `"back"`),
    /// for in-process `WKWebView` / `WKURLSchemeHandler` loading without loopback HTTP.
    pub async fn get_anki_card_face_html(
        &self,
        card_id: Uuid,
        side: &str,
    ) -> Result<String, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::render_card_face_html(&self.state, card_id, side).await
    }

    /// Returns the stored media MIME type and bytes for an imported Anki package fingerprint
    /// (64-character hex string in `anki_media.package`) and asset `name`, for in-process
    /// `WKURLSchemeHandler` media serving without loopback HTTP.
    pub async fn get_anki_media_blob(
        &self,
        package: &str,
        name: &str,
    ) -> Result<AnkiMediaBlob, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::load_media_blob(&self.state, package, name).await
    }
}
