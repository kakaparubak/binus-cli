# Explicitly approved final Courses seam/test correction

User was told two fix passes had been used, final review found no reachable security issue/new production logic bug, but exact test-only seam constraint and regression gaps remained. User explicitly said 'yes go' to ONE final strictly scoped correction on gpt-6-luna. Implement ONLY below in /home/abdel/Documents/Coding/binus-cli/spikes/001-auth. No newfeatures, no commits/push, no credentials/liveAPI/browser/sourcewrites, no auth/storage/guard refactor. Keep working production fixed-HTTPS GET flow and schema unchanged. Parent independently reran35lib+8CLI tests/fmt/clippy/build and liveperiods3/courseperiod2610=10unique3LAB7LEC complete/noerrors. Actualsavedauthworksoutside sandbox; don'ttouchownerchecks to fixnamespaceuid65534 issue.

## Final independent review exact gaps

1. Testoriginseam is private BUT compiled into RELEASE: courses.rs periods_with/list_with accept Option<(&str,Duration)> and request_at rewrites authenticated URL to suppliedorigin. Public entrypoints alwayspassNone; no externaloverride/exfiltrationpath found. Still violates approved cfg(test)-only alternateorigin. Gate alternate-origin configuration under #[cfg(test)] while keeping SAME shared production orchestration for tests. No env/CLI/config/public-origin override. Recommended small config enum with Production variant and #[cfg(test)] Loopback variant; validate testURL actually loopback. Or cfgfield/internalconfig getter; choose smallestclear design. Normal build MUST lack ability to supplyalternateorigin/testlimits. Avoid duplicatefakepipeline, unsafeprodTLS, or broadrefactor. Keep live defaultrequesttimeout20s/overall60s fixed. Existingtests compiledcfgtest use loopbacktransport+shortinternaldeadline.

2. ActualduplicateCLASSROW is missing in supposeddedupetest (courses.rs508-530): currently verifies duplicate COMPONENTlabels, LAB/LEC samecourse/class IDs, additionalSEM, but no repeatedclassrow. Add exactduplicaterow fixturewithincomponent and assertoneclass and coveragecomplete, while distinctcomponents stayretained. Keep duplicatecomponentcoverageexistingtest.

3. Retained-DATA partial missing: existingmatrix tests LABempty+failure but not validLABrow+LECfailure. Add assertpartial/exit3, validrowretained, success/failedcomponentstatuses, completefalse/discoverytrue. Add malformed atomiccomponent containing onevalidrow THEN malformedrow plus successfulsibling: failedcomponent rowsALLrejected, siblingvaliddataretained andpartial. Sameforcomponentconflictifparameterizingeasy. Existingwholecomponentatomicbehavioraccepted; don'trewrite per-itemsemantics.

4. Timeouttests currently immediateheaders/body-stall ONLY. Extend mockfixture to allowdelayedheaders AND delayedbody separately. Add actualsharedpipeline tests:
- delayedheadersrequesttimeout (shorttestlimit, no20/60s waiting)
- delayedheaders+body combined exceed one absoluteoverallbudget though eachphase individually under it; testmustfailold reusedbudgetversion
- discovery andmultiplecomponentresponses consume one sharedtotalbudget (notnewbudgetpercomponent), assertcorrectpartialerror/coverage andtimebound.
Use deterministic generous timing windows/localserver fixture; noflaky50ms expectexact scheduler. Keepactualsameget_at/listpipeline under test. Timedserver threads mustnotpanic unnecessarily on clienttimeout/closedconnection and mustcleanup.

Nonblocking review suggestions may fix cheaply in same tests ONLY: assert exactinvalid_jsonvsinvalid_schema codes, capturedHTTPbodyempty notjustabsence of'{', timestamps near currenttime notmerelyafter2025/year. Do not expand scope or addnewfeatures/dependencieswithoutneed.

## Execution discipline

FIRST add failingtests wherebehavior/protections needcorrection then minimalimplementation. Don't returnanotherreport saying testsstillneedadding; deliverexact4gapfixes or realblocker. No source/publicbundle rediscovery needed, schemaalreadyverified. No liveAPIinsideCodexsandbox. All fixtures synthetic, capturedauthsynthetictokens only. No realclassrecords inrepo/notes.

Run cargo test, cargo fmt --check, cargo clippy --all-targets -- -D warnings, cargo build and cargo build --release. Inspect release-facing source/config to confirm test-originvariant/parameter absent from normal build; releasebuildsuccessalone doesn'tprove this, explain exactcfgstructure. Keepauthtests bothbinaries. Reportactualtestcounts/regressions andexactfiles, no claims of post-change liveverify (parentwilldoit).

Write DETAILED evidence to parentrepo `.agent/courses-final-seam-evidence.md`. The CLI -o final summary uses a DIFFERENT filename `.agent/courses-final-seam-summary.md`; do not overwrite detailed evidence or selfreference final summary. Include finalchanges/actualchecks/remaininglimits concisely. Stopafterthisscope; parentwillfinalreview+hostsideAPIread. No auto-nexttask or newcapabilities. Retain modelgpt-6-luna even ifcapacityerror; preservesavedwork.
