# Courses slice: independent review fix and missing acceptance tests

Continue /home/abdel/Documents/Coding/binus-cli, model gpt-6-luna. User approved bounded Courses periods/list. Your implementation now LIVE VERIFIED by parent normalSSH: courses periods3 catalogIDs; courses list --period2610 10unique enrollments3LAB+7LEC; selectedperiodonly/bothcomponentscomplete/noerrors/timestampsparse. Parentindependently ran34tests/fmt/clippy/build passed. Don'tredoauth/schemaprobe/asklogin. Sandbox /homeuid65534 issue understood; no actualcredentials/liveAPIinsandbox. Parent rerunsactualcommandsoutside afterthispass. No commits/push, no writes toBINUS/coursework/tokenstore, no newfeatures/SQLite/offline/refresh.

Independent source review FAILED with3 concretebugs plus requiredtestsmissing. Fix ONLY these and related testseam/clienterrorhandling; do not rework wholeworkspace. Retain verifiedschema (period academicPeriod, componentstringarrays, class crseId etc), sourceGETfixedorigin, roleheaders/noCookies/query/body/redirect, sharedauthCLI, passivecapture/securestorage.

## Actual defects (parent inspectedservice andreviewerinspectedCLI)

1. courses.rs249-270 get(): calculates remain once then resets same timeoutbudget forclient.execute(headers) andresponse.bytes(body). A late-startrequest canovershootabsolute60s despiteperreq20s. Use ONE absolute overall deadline and perrequest deadline with timeout_at across headers+body; don'trefreshbudgetatresponsephase. Or recompute remainingbeforebody (but ensure oneoveralltotalbudget). Testsdelayedheaders+delayedbody; shorteninternaltestlimits deterministically so not60s tests. Same productionexecutionpath should exercise header+bodydelays and failwithin absolutebudget. Returning timeout must report component/coveragecorrectly.

2. cli.rs74-84 loadscredentialsbeforevalidating suppliedperiod. Missingflag alreadyClapusage2 but `--period ../x` withnocreds emitsnot_logged_in. Put safe_segment validator inClapvalue_parser or immediatelyafterparse BEFOREload_preferred_record. Unsafeinput usageexit2 andno credential/networkaccess. Tests with missing/insecuretmpcredentialdir provevalidationwins. ValidcatalogIDs e.g2610 unaffected. Existing safe_segment denies slash/query/emptydotsegments; preservepolicy.

3. cli.rs168-173 missingcredenvelope timestamp hardcoded1970;serializationfallbackline160 same. ShareactualUTCtimestamp/envelopeconstructor withcourses forERRORpaths too. No inventedfixednow except injection forunittests. Timestamp regression mustcheck near realcurrenttime not merelyvalidformat. Existing homegrown timestampformatter should havebasic known-boundarychecks orreplacewithlegitmaintainedtimecrate if simpler, but no gratuitousdep.

4. coursesclient() currentlybuild.expect -> canpanic. Return sanitized client_setup_failed envelope likeauthalreadydoes; testconstructionerrorviainternalfactoryseam iffeasible without introducingarbitrarypublicendpointconfig. Keep errorsJSON structured.

## Required acceptance tests (FIRST failing then fixes)

Currentcoursesmoduleonly2parsertests andCLIonly1newusage/crederror test; originaltaskexplicitlyrequires REAL serviceorchestration+CLIserialization/exitbehavior. Need controlled internaltransport/service seam while PRODorigin staysfixed. No arbitrary --base-url/envproxytestflag/customrealcredentialforwarding. Use private/cfgtest transport or localHTTPmocksender/client-injection withproductiondefaults notexternallyoverrideable. Can injectprivate limits for shorttimeouttests. Testsynthetic tokens ONLY in sandboxownedtmp directories. Do not expose realcredentials ormodifyprotectedloader fornamespace.

Testactualserviceperiods/list orchestration, notjustcallmapparsers orbuildmanualenvelopes:
- Complete discoveredLAB/LEC/additionalSEM, allfetched; actualGETpaths/header rolefields/Bearer, noCookies/query/body; selectedperiodencoded/validated andpropagated.
- Emptyperiodarray, emptycomponentarray, andemptyclassarrays returncompleteempty.
- Onecomponentfails, anothersucceeds ->partialdata/coverage; specificallysuccessEMPTY+failure ->partial, noterror orcomplete. Allcomponentsfailed ->error; discoveryfail ->discovery_completefalse. Invalidcomponentdiscoveryrootobject ->error, noallclasscalls.
- HTTP401403reauth vs500upstream; invalidJSON; unsupportedresponseenvelope; malformedrow/componentconflict ->explicitfailedcomponent. Currentcomponent-atomicmapping acceptable: doNOT silentlylosevalidrowswhileclaimcomplete; document/testthatonebadrowrejectsentirecomponent.
- Networkfailure/header/body/absolute deadlines; redirectrefusal and NO redirectedAuthorizationforwarding; ideally localmocktransportprovesredirectnotfollowed.
- Dedupe actualserviceoutput by(period,component,classId), retaindistinctLAB/LEC samecourseID, additionaldiscoveredlabelnotignored. Duplicatecomponentlabels shouldnotcauseinconsistentcoverageorunboundedrepeatreads; dedupeorerror visibly.
- Actual sharedCLIcommandexecution/serialization layer over syntheticservice results -> exactexit0complete/1error/3partial, matchingJSONschema/data/coverage; notmerelyteststatushelper. Can exposeinternallyparameterizedcli run_from with syntheticservice/credentials/output undercfgtest; no productionfakeAPIsettings. Atleastbothbinaryauthstatus/checkmissing-credsregressions (oldtestsmostlyonlyoldbin), invalid--periodusage2beforecredentials, actualerrortimestamp notepoch.

Avoid broadrefactor; one focused service dependency seam is justified for testing currentlyuntestablefailures. Allauthregressions muststillpass. Testsneed showassertedfailuresemantics not cosmeticcountinflation. LabelsyntheticfixturesactualsyntheticanddoNOTreportthemasliveevidence.

## Verify and handoff

cargo test, cargo fmt --check, cargo clippy --all-targets -- -Dwarnings, cargo build, bothhelp. No liveAPIinsandbox becauseknownnamespaceproblem. Reporttestcounts/testcasesactuallyrun, exactfilesandcommandsandremaininglimits in .agent/courses-slice-result.md; don'toverwritefullreportwithselfreferencingplaceholder. Liveparentresultknownabovebutpostfixliveverificationpendingparent. Update README tiny notespartial/error/exitcodes/componentatomic/errorgeneration andnewboundstimeout truthful. No newunexpectedpermissions/storagechange. Parentwillindependentlyinspect/runliveperiods+2610againandreview. Scope onefix/testpass, notnextfeatures.
