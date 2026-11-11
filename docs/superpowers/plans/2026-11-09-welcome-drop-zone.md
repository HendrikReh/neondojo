# Welcome match import drop zone

Approved design: below Open match/Import match, spanning their combined width, show a rounded110px drop zone with light surface, dashed gray border, small file/down-arrow icon, localized title and MAT · XG · SGF · BGF. File drag activates blue border/tint and release-to-import title. Keep nested drag events stable and reset on leave, cancel, drop or blur. Text drags remain neutral. Use existing import callback and busy guard; normal import button remains the keyboard-accessible file chooser.

Implementation: isolated MatchDropZone component, shared welcome grid placement, six complete localized labels, preserve existing broader welcome drop behavior. Browser checks: layout, nested drag state, cancellation, all four extensions, single import/busy guard, errors and mobile width. Then build and deploy tested static assets locally; verify live UI without API writes.


Completed and deployed locally. New MatchDropZone handles file-only drag state, nested leave/enter, window blur/dragend/drop reset, busy feedback and existing importer routing without bubbling duplicate drops. All six locales include normal/release titles. Verification:11 browser cases (6 drop-zone plus5 home navigation),8 localization tests, successful build and visual geometry checks across12 locale/viewport combinations. Independent review had no actionable findings. Deployed UI was rechecked read-only on8787 with zero API writes. Screenshots, previous index and report are in ignored backups/welcome-drop-zone-20261109. Task-owned preview5199 stopped.
