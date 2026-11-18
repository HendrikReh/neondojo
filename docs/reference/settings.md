# Settings, engines and profiles

How settings, engine installations, configuration profiles, themes and languages behave. For a guided introduction, see the [user guide](../user-guide/README.md#9-profiles-and-settings).

## Settings and engines

**Settings** opens a dialog with **General**, **Appearance**, **Analysis**, **Playing**, **Board**, and **Profiles**. The **General → Language** menu lists Deutsch, English, Español, Français, Italiano, and Polski alphabetically. New installations default to English. **Board** offers previews of Forest, Walnut, Slate, and local BGBlitz themes from `themes/*.zip`. **Apply** saves the draft and keeps the current category open. **Close** returns to the workspace. Closing the dialog, pressing Escape, or opening Profiles, Opening books, or Benchmark with unapplied edits offers **Apply**, **Discard draft**, or **Keep editing**. In the category list, Up and Down move to the previous or next category, and Home and End to the first or last. **General** holds the language and the player name. **Playing** holds the bot engine and strength, the default play mode, **Open training analysis automatically**, game sounds and the bear-off direction. **Analysis** also links to **Benchmark**. [Configuration profiles](#configuration-profiles)

Set a player name, default analysis depth (GNUbg 1–5 ply, default 3), separate playing strength (GNUbg 1–3 ply, default 1), and bear-off direction. New matches use the playing settings in effect at creation. Choose GNUbg or Open Sage independently for analysis and playing. The pinned Open Sage fork supports 1- and 2-ply for play and up to 4-ply for analysis. The original wheel does not pass the 1-ply capability check. New sessions pin the bot, analysis and GNUbg rules installations separately. There is no automatic fallback to another engine.

Open Sage requires a locked native ARM64 Python 3.12 environment. In **Settings → Analysis → Engines on this computer**, enter that environment's `bin/python` path and choose **Verify and save**. The backend checks package and model integrity and runs a real evaluation before enabling selection. Installation paths stay on the host. GNUbg handles MAT conversion and provides the play rules, dice, and SGF handling. Open Sage makes bot decisions when selected. Open Sage 1-ply requires a successful capability check with a bounded thread count. Open Sage 3- and 4-ply are available for retrospective analysis and engine duels. New jobs and duels probe the requested profile before starting. [Local Open Sage setup](../open-sage-local.md)

**Settings → Analysis** shows the default analysis engine and depth, the error thresholds, and the configured opening-book source. Set the four inclusive lower bounds in normalized equity loss. They must be strictly increasing, greater than zero, and at most one.

| Grade | Default threshold |
| --- | ---: |
| Questionable | 0.005 |
| Error | 0.020 |
| Blunder | 0.080 |
| Large Blunder | 0.120 |

The thresholds are part of the current configuration and can be saved in a profile. Applying changes updates the classification of existing analysis results from their stored equity losses. New matches capture the scale for feedback.

Expand **Engines on this computer** to see the local installation paths and set the maximum calculation threads per engine, separately for analysis and for playing. The collapsed section shows availability for both engines. The backend detects the logical CPUs available to its process and probes how many of those GNUbg accepts. The dialog offers whole-number overrides within the detected range, starting at 1 for GNUbg and 2 for Open Sage. GNUbg defaults to 1 for both purposes. Open Sage defaults to 4 for analyses and engine duels and 2 for playing. The analysis value applies to new analysis jobs and duel series, the playing value to the bot of new play sessions. Analyses of a play session use the analysis value. Each job, session and series captures its thread counts when created. **Simultaneous analysis jobs** sets how many position, dice equity and match analysis jobs run at the same time on this computer, from 1 to 8. The default is 1. A change applies when the next job starts; running jobs continue. Open Sage analyses share one sidecar process, so at most one runs at a time while GNUbg jobs use the remaining slots. Each job uses its own analysis threads, so jobs × threads should stay within the logical CPUs. Engine duels and play sessions have their own workers and do not count. Remaining-time estimates come from benchmarks of a single job and can be too optimistic while several jobs share the CPU. Reference benchmarks retain their fixed thread counts, and their timings are only used for estimates with a matching thread count.

Automatic result readers stay within the selected analysis provider. Historical results and explicitly selected jobs retain their original provider and profile; copying a match retains completed results from both providers. Sage results carry model/profile identity but do not claim GNUbg PR statistics.

The executable path is specific to the backend computer. Installation checks, analysis, playing, and benchmarks use the same resolution rules. An absolute path is resolved directly, a relative path containing a directory separator is resolved from the backend's working directory, and a bare name such as `gnubg` uses the first matching file on the backend's `PATH`. The backend reports an error if resolving or running that file fails. The resolved path must be valid UTF-8.

**Check installation** and saving a changed executable path check the version and run a real position evaluation within a single 10-second time limit. A failed check prevents an invalid path from replacing the saved installation. Installation settings use PostgreSQL revision checks. A concurrent change surfaces a conflict and retains the draft.

A new analysis job records its engine configuration and effective ply. Active-job deduplication includes the target, scope, ply, configured executable path, and version. An engine version change at the same path fails a job that expects the original version and preserves completed game checkpoints. Legacy jobs without a snapshot use the engine configured at backend startup. MAT imports use the currently saved executable path.

### Ply convention

BG Tutor stores, shows and compares every depth in the XG and Open Sage convention: **1-ply is the raw neural-net evaluation**, and each further ply looks one roll ahead. GNUbg numbers the same raw evaluation 0-ply, so BG Tutor's 3-ply is GNUbg's native 2-ply. The backend converts only where it writes commands for GNUbg (`engine::gnubg_plies`), and converts depths that GNUbg reports back. A GNUbg 3-ply and an Open Sage 3-ply duel side therefore search to the same depth.

Migration 0079 moved every stored GNUbg depth up by one level: saved settings and profiles, jobs and their results, match checkpoints, benchmarks, duel series, play sessions, flashcards and imported GNUbg SGF evaluations. The raw source tokens of imported analyses stay unchanged. Archived duel SGFs, and the player names and events read from them, keep the text written when the match was played. In duels from before migration 0079, a GNUbg side named `gnubg 2-ply` ran at today's 3-ply. Finished practice attempts, reviews and study-report snapshots also keep their recorded text. Flashcards exported to Anki before migration 0079 carry GNUbg's native numbering.

Migrations 0019 and 0020 added GNUbg's native 3- and 4-ply analysis, today's 4- and 5-ply. Explicit profiles, existing jobs, and completed results retain their recorded depth. The requested depth is the GNUbg search profile, which allows early exit when the result is clear. Higher depths take longer, especially for full matches and dice distributions.

New GNUbg 3- and 5-ply analysis jobs save a checker move filter that sends up to the best 14 candidates from the 1-ply evaluation to 3-ply. At 5-ply it also sends up to the best 14 candidates from the 3-ply evaluation to 5-ply. Game and match analyses retain the actual depth of each exported candidate. Existing jobs keep the filter saved at submission or the GNUbg startup filter for older jobs.

## Configuration profiles

The header shows the active profile and marks it **modified** when your current settings differ from its saved values. Select a profile to switch or choose **Manage profiles** to open **Settings → Profiles**.

Choose **Show changes** in the profile menu to see each changed setting with its category, saved value, and current value. The arrow next to a current value restores that field from the saved profile and saves the current configuration. Related values such as a dice pair or a numeric or date range are restored together. The dialog shows any errors. The **modified** marker disappears after the last change is reverted. **Close** or Escape dismisses the dialog.

**Reset all changes** restores the entire saved profile in one operation after confirmation. **Cancel** or Escape dismisses the confirmation. A failed restore preserves the current configuration. After success, the changes dialog stays open with an empty list.

Profiles capture engines, default depths and error thresholds, playing and training defaults, language, sound, interface and board appearance, and opening-book references. **Settings → Profiles** shows the active profile and its change count at the top. Select a profile to compare it with the current configuration. Its full saved configuration can be expanded below the comparison. Profile timestamps use the app language and the browser's local time zone.

Each browser tab keeps its own database selection, search filters, sorting, Study deck, card-creation choices, and analysis filters. Table columns, column widths, and analysis board layouts are stored in the browser. Both sets of preferences survive reloads and profile switches. The browser uses legacy configuration values to seed these preferences once. Older profile files retain those values for compatibility. A removed database or deck is handled in the workspace that uses the selection.

**Apply** saves the current settings. To update a named profile, open **Settings → Profiles** and choose **Save in profile**. **Save as new profile** asks for a name, then creates and activates a profile. The **Profile actions** menu contains **Rename** and **Delete**. Activate a different profile before deleting the active one. **Restore saved profile** reapplies the active profile's saved values after confirmation. To save changes while switching profiles, use **Save and switch** in **Settings → Profiles**.

On the first launch after the profiles update, existing server and browser preferences are imported once into **Standard**. Profiles and current settings apply across the installation, including other browsers. On a version conflict, you can load the server version or save your local changes as a new profile. A recovery journal protects pending local changes.

Running analyses and matches retain their captured settings. If the profile cannot load, workspaces use the last confirmed settings or the built-in defaults. The profile controls report the failure and offer a retry. Engine installation paths are device-specific, and book files are stored as references. Activation checks the referenced resources and reports missing items. See the [original profile specification](../superpowers/specs/2026-11-10-profiles-design.md) and [implementation plan](../superpowers/plans/2026-11-10-profiles.md).

## Themes and language

### Board artwork

`npm run dev` and `npm run build` prepare artwork from locally supplied BGBlitz archives. Preparation requires **Python 3.10+** and uses the original backgrounds, checker variants, dice, cubes and point-number strips. Place new archives in `themes/` and restart the dev server or run `npm run themes:prepare --prefix web`. The three built-in themes are always available. [Board theme formats, sources and rights](../../themes/README.md)

### Interface appearance and language

Under **Settings → Appearance**, choose **Graphite**, **Club** or **Paper** and a **Light**, **Dark** or **System** display mode. System follows the operating system. On desktop, the interface preview updates to the right of the controls. Under **Settings → Board**, scroll the gallery on the left to browse artwork while the selected board stays visible on the right. Use **Apply** to save both previews. The browser applies its saved appearance before the first render. Tables, review sessions, dialogs and error messages use the same appearance.

Use **Apply** to save draft language and board settings. To discard a preview, choose **Close** and then **Discard draft**. BG Tutor localizes the interface, status messages, errors and number formats while preserving visible analysis, player names, filenames and GNUbg move notation. Native engine diagnostics accompany localized errors. All six catalogs are type-checked and their keys and placeholders are validated at startup. Counted nouns use plural blocks such as `{count, plural, one {# card} other {# cards}}`; `web/src/i18n-core.ts` chooses the form with `Intl.PluralRules`, and Polish supplies `one`, `few` and `many`. The translations are pending native-speaker review.

### Action icons and tooltips

The app uses a shared Lucide Outline icon set for actions such as saving, undoing, requesting hints, searching, and importing or exporting. Compact actions have localized accessible names and show tooltips on hover and keyboard focus. Key play decisions, **Undo step**, **Hint**, primary form submissions, and study actions also show text labels.

Tooltips stay above modal dialogs and remain visible when you move the pointer onto their text. Escape dismisses the tooltip and preserves the dialog's Escape behavior. The shared implementation lives in `web/src/action-icons.tsx`, `web/src/ActionButton.tsx`, and `web/src/useActionTooltip.tsx`. Action buttons preserve native submit and disabled behavior and forward refs. Export controls are download links.
