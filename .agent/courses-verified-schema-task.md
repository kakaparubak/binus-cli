# Resume APPROVED Courses slice: actual schema now verified host-side

Work /home/abdel/Documents/Coding/binus-cli on gpt-6-luna. Implement original approved `.agent/courses-slice-task.md`, using this fresh ground truth. User approved bounded periods+courseslist slice. Helper investigation is COMPLETE; don't spend another pass probing or stop because sandbox cannot accessstoredcredentials. Parent will execute newcommands via NORMAL SSH after coding. All implementation code remains here, no commits/push, no coursewrites, no browser, no tokenrenewal.

## Actual observed schema (parent ran built helper OUTSIDE sandbox)

GET /func-bm7-course-prod/AcademicPeriod/Student HTTP200 ROOT ARRAY of3 objects:
academicPeriod:string values2610/2520/2510 (catalog IDs, no fulluserrecords)
academicPeriodDescription:string
academicPeriodId:null (therefore DO NOT use academicPeriodId as pathID)
academicPeriodStatus:boolean
isActiveRunningPeriod:boolean
isRunningPeriod:boolean
termBeginDate:string,termEndDate:string

Explicit test period2610 chosen, not claimedcurrent/active. GET /Course/Period/2610/ClassComponentList/Student HTTP200 ROOT ARRAYOFSTRINGS ["LAB","LEC"]. Each string is actual component value for request, not object id/name.
GET /Course/Period/2610/Component/LAB/Student HTTP200 ROOT ARRAY3 courseobjects; /Component/LEC/Student HTTP200 ROOT ARRAY7 objects. Same observedfields/types acrossboth:
classId:string, classCode:string, classGroupId:string,classNumber:integer
courseCode:string, courseName:string, crseId:string (course sourceID!), ssrComponent:string
progressClass:integer
courseTypes:arrayobjects courseType:string, courseTypeId:string
courseFlags:arrayobjects courseFlag:null|string,courseFlagDesc:null|string,courseFlagId:null|string
lecturers:arrayobjects id/name/userCode/pictureUrl/order/subRoleTypeId/subRoleTypeDescription

Only emit lightweight desiredfields: class_id<-classId,course_id<-crseId,course_code<-courseCode,course_name<-courseName,class_code<-classCode,period<-selectedperiod,component<-ssrComponent (mustmatchdiscoveredselection or reportconflict). DO NOT expose lecturerIDs/names/pictures or derivedprogress/completionstatus; classGroupId/classNumber notneededforfirstslice. Periodoutput map stablecatalogfields (period,description,running flags, termdates preserve naive sourcevalues no assumedtimezone). Requiredfieldvalidation not silentlyfallbackmissingIDs to arbitraryotherfields. Public frontend previously confirmed flow selectedperiodvalue -> componentarraystrings -> classarray. Arrays have no continuationenvelope in testedsource; reject unsupportedrootobject/pagination-envelope as invalid_schema/explicitincomplete rather than assumingfirstnestedarray. Don't silentlytruncate or hardcode10 counts. Programmatic dedupe key(period,component,classId), retainLEC+LAB even samecourseid. No studentrecords/testfixturesfromlivevalues.

## Sandbox mismatch is RESOLVED operationally, not a productbug
Sandbox presents /home uid65534; normalhost uid0. secure_parent rejects65534correctly. Never weakenowner checks, disable sandbox, copytokenfile, changepermissions, put credentials in env, or askrelogin. Build/test only with syntheticauth insandbox. REAL verification will be performed by parent outside sandbox using current savedpair. Your result must state pending-parent-live-check, not claimerrorblocker thatprevents completing code. Parent just successfully did helperreads; credentialsvalid. Example helper nowexists examples/course_schema.rs; preserveor clearlylabeldiagnostic, notproductioncommand.

## Deliver exact bounded interface

`binus courses periods`
`binus courses list --period <catalogID>`
Reuseexistingcrate spikes/001-auth; retain old binus-auth-spike binary/authcommands and tests. Add sharedCLIimplementationwiththinbinentrypoints, no duplicatedauthlogic/includehack/wholeworkspaceapp. Keep authcredsdirpathunchanged. Preserveauthguards/passivecapture/storage/signals. Parent uses `cargo run --manifest-path spikes/001-auth/Cargo.toml --bin binus -- ...`. Keepdefault-run compatible. Scope NOsessions/forums/assessments/cacheSQLite/offline/refresh yet.

Fornewcoursescommands use specminimalJSON envelope schema_version1, statuscomplete/partial/error, data[], coverage.discovery_complete/complete/components statuses, errors sanitizedstage/component/httpstatus, warnings[], fetched_atUTC. Missing--period usageexit2 beforecredentialread. Complete0,error1,partial3; preserveexistingauthcodes. Mixedsuccessfulcomponent+failurepartialevenifsuccessfulzero; alldiscoveredfetchesfailed error; genuineemptycomplete. Malformeditem explicitpercomponenterror/incomplete; discoveryfailurefalse. 401/403 reauth_required,500upstreamerrornotexpiry. Finite perrequest20s andabsoluteoverall60s; fixedallowlistedproductionGETorigin, nofollowredirects/Cookies/subscriptionquery, correctBearer+roleHEADERS. Reuseoriginaltasksecurityandtestrequirements; don'taddarbitrarybaseURLoverridefortestsorproduction. Encodepathsegments/rejectquery/slashdotsegments toavoidpathescapes.

FIRST failing syntheticfixture tests then implementation, usingactualschemaabove. Test periodarray mapping, componentSTRINGarray/allreturnedlabelsnotjustLEC/LAB, actualclassfieldmapping, dedupewithdistinctcomponents, completeempty/partialmixedempty/allfailed/discoveryfailed/malformeditems/JSON/http401403500/timeout/unsafeperiodpath/redirectrefusal. TestserviceandCLIactualenvelope/exitpaths/missingcreds (syntheticauth/tmpconfigs must reside sandbox-owned paths). Use testtransport/localmock injection not arbitraryproductionoverride. Existingauthregressionsmustpass. Do notskiptests just because readonlyschema waspassedmanually.

Run cargo test,fmt --check,clippy --all-targets -- -Dwarnings,cargo build andbothbinhelp. Return exactsourcefiles/artifactpaths/testcounts/commandsthen `.agent/courses-slice-result.md` forparent. No livereadinsandbox, don'tblock/quitimplementationonknownsandboxauthstorageissue. No output realsecrets/enrollments. Parent will reviewcode and run newcommandsperiods+period2610 with countverificationoutside sandbox. If modelcapacitydoNOTswitch; savesmallincrementalchanges. Focus actualworkingartifact notanotherplan/reportonly.
