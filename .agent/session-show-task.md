# Approved implementation: selected session SHOW topic/material metadata

Work /home/abdel/Documents/Coding/binus-cli, modelgpt-6-luna. Userapprovednext bounded `binus courses sessions show <session-id>` for topictext/materialmetadata, NOdownloads/materialURLfollowing/progress/sourcewrites. PriorCourses/Sessionsaccepted; current50lib+9CLI+helpertestsgreen. Parentbuilt/runnewdetailhelperOUTSIDE sandbox andverifiedexactschemasbelow. Noactualcredential/API/browser insideCodex; /homeuid65534sandboxissueknown, don'tweakenloader/copytokens/refresh/signin. Parentwillrunrelease outside afterwards. No commits/push ornewfeatures/cache/forum/assessment/scheduleoperations.

## Verified live reads and constraints
Parentusedreleasecourseperiod2610 rows sorted(course_code,class_code,class_id), choseFIRSTLECandFIRSTLABclass, thenFIRSTsession fromreleaseSessionslist. For eachsession called:
primary GET https://apim-bm7-prod-web.azure-api.net/func-bm7-course-prod/ClassSession/Session/{id}/Resource/Student?isWeb=true
additional GET samebase/ClassSession/Session/{id}/AdditionalResources
All4HTTP200. LECprimaryresources5/additional1,LABprimaryresources4/additional1. Noobservedpaginationfields. Thesearesamplecountsnot universalhardcode. A fewpreceding course/sessionreads returned transientnetwork_error; authcheck remainedsuccessfulandboundedretryworked. Don'treinterpret500/networkasexpiredtoken oraddautomaticrefresh. Productshowshouldhonestlyreportsuchfailures/partial, nothidesilently.

Primary rootOBJECT fields observed:
academicPeriod:string,classCode:string,classDeliveryMode:string,courseCode:string,courseTitleEn:string,crseId:string
courseSubTopic:ARRAYSTRINGS7bothsamples
topic:string,sessionNumber:integer
learningOutcomes:arrayobjects1LEC2LAB {id:string,index:integer,isStronglyRelated:boolean,learningOutcome:string,errorMessage:null}
dateStart:string,dateEnd:string,startDateSessionUtc:string,endDateSessionUtc:string,meetingStart:string,meetingEnd:string
deliveryMode:string,deliveryModeDesc:string,facilityId:string,roomDescription:string,isEnded:boolean
resources:arrayobjects5LEC4LAB,totalResource:integer
classSessionProgress:object{completed:int,inProgress:int,notStarted:int},progressByTab:int
lecturers array (IGNORE), attendanceDateTime/status/wifiAttendanceDate/onlineConfirmationDate (IGNORE),joinUrl:string(OMIT)

Each PRIMARY resources[]:
id:string,classSessionId:string,resourceId:null|string,name:string,resourceType:string,type:null|string,index:int,tabIndex:int,tabName:string
startDate:string,dueDate:string,submittedDateFirstAttempt:string,lastUpdatedDate:string,resourceLastUpdatedDate:string
contentLastModifiedUtc:null|string,contentLength:int,duration:string
progressStamp:int,progressStatus:int,resourceStatus:string,timesAccessed:int
isAssessment:bool,isAvailableOffline:bool,isLateAllowed:bool,isOpen:bool,isOverdue:bool,isSubmitDHD:bool,isZoomAvailable:bool,isReport:int,hybridType:string
url:null|string,token:null,thumbnail:null,androidRedirectUrl:null,iosRedirectUrl:null,assessmentType:null
warningWordings:null|arraystrings,wording:string

Additional root ARRAY1each,objects:
id:string,classSessionId:string,resourceId:string,name:string,resourceType:string,index:int,tabIndex:int,tabName:string,type:null,url:string,generatedDate:string,isOpen:bool.
Nootherfieldsestablished. Beware resource.id vs resourceId are DIFFERENT, latternullableprimary; preserveboth anddon'tfallbacknullresourceIdtoid. OnlyquerythesetwoURLs, neverfollowembeddedurls.

## Bounded output/behavior

NewSHOW nestedsubcommand accepts positional sessionID fromlist; usage/malformed/unsafeIDexit2 BEFOREcredentials; --help noI/O. Reuseexisting safesegmentpolicy/headerbuilder/auth/sharedCLIserializer andcurrentUTCtimestamp, don'tforkauth/path. ProductionfixedHTTPSGETonly; primaryONLYqueryisWeb=true (notsecret), additionalnoquery, noCookie/body/subscriptionkey. DisableHTTPredirects forboth; no resourceURLlookup/download/browser. ONE20sdeadline perrequest inclheadersandALLchunks; sharedwhole-show60sdeadline acrossatmost2requests;8MiBstreamedcap/request;cfgtest-onlyloopback/limits, no productionorigin/envflagoverride. Don'treintroducepreviousdeadline/seamregressions.

CommonJSON schema_version1,statuscomplete/partial/error, data OBJECT|null forselectedsession, errorssanitizedstage/source/sessionid/httpstatus,warnings[],coverage explicitselectedsessionscope+primary/additionalstatuses/discoverycomplete/complete. Existinglistperiods/courses/sessionsJSON mustremainUNCHANGED. Complete0,partial3,error1,usage2. Ifbothendpoints valid =>complete;onefails/schemaunsupported =>PARTIAL retainingOTHERsuccessfulendpointdata, explicitfailedsource/error andunknownnullprimaryfields notemptyfakefacts;bothfail=>error/nousabledata. Emptyadditionalarray islegitimatesuccess, primary resourcesempty withvalidtopicmetadatalegit. Don'treportemptywhenrequestfailed. Ifsourcepartialduecount/pagination don'tclaimcomplete. Endpoint-atomicmappingforbadrowaccepted: markthatendpointfailed, retainothersource; no hiddenvalid-primarytopic claimedcompleteonresourceparsefailure.

Data minimalusefulfields (notentireupstreamdump): session_id; primary-only topic/rawsubtopics andlearning_outcomeTEXTS (optionalverifiedmetadata, notlectureridentity); course_code/course_title/class_code/component(period academicPeriod)/period; source_session_number integer; start/end dates anddeliverycode/description. Preserve rawnaivedates andUTC-labelledstrings withoutinventingtimezone. Year0001-01-01 sentinels ->nullunknown andretainraw source_*date values whereexposed. DoNOTclaim sessiondate=assignmentdeadline or rawprogressints=submitted/completed.

Primaryresources andadditionalresources areSEPARATE namedarrays indata, nullwhenfailednot[]. Resource descriptorsminimal stableIDs(id/session_resource_id,resource_idnullable,class_session_id),name,rawresource_type/rawtype,index/tab_name,is_open,has_link BOOL (urlnonempty) butNOURLORtoken. Primary mayinclude source_due_date(normalized+raw),content_length asSOURCEvalue notverifieddownloadfilesize,source_progress_status/progress_stamp/resource_status/times_accessed/last_updated_date asRAWsourcefacts (usefulparentbeforeaftermetadataread check), notnormalizedsubmission/completion. Additionalfieldsnotpresent stayabsent/null notinventedprimaryprogress. Don'temitwording/redirectURL/joinURL/thumbnail/tokens/rawsignedlocators/lecturer/student/attendancefields. Actualtopic/subtopic/outcometextallowedasrequestedusercontent, treatedasJSONdata neverexecutedHTML/JS. Noactualenrollment/textfixturesfromliveAPI inrepo/vault.

Completeness: rootprimarytotalResource is integer butscalarcountVALUEwasnotloggedbyhelper. Comparedeclaredcountandactualprimaryresourcearray whensemanticallyappropriate; ifmismatch makevisible source_count_mismatch/incomplete (don'tsilentlytruncate orguesspagination). Noobservedpagination; nonemptyrecognizedcontinuation/truehasNext indicators ->unsupported_pagination/incomplete, neverfollowURL. Additionalrootarray only (notfirstnestedarray). ResourceclassSessionId shouldmatchrequested; ifmismatch explicitschema/contextconflict. Dedupeexactduplicates by(source,sessionid,id),conflictingduplicatesexpliciterror; nevermergeprimary/additionalidentityaccidentally. No hardcoded5/4/1 counts.

## FIRST failing real tests then implementation; avoid prior missing-test loops

- Exactverifiedprimaryobject+additionalarraymapping, nullableprimaryresourceId distinctid, unknownresourceType rawretained, allURL/tokenfields omitted evenwithsyntheticsignedvalue; nolecturer/attendanceleaks. Topic/subtopic/text preserved. Sentinel/naivedates noTZguess. Namedarraysonly, malformedroots/rows/pagination/countmismatch/contextmismatch/exactduplicate/conflictingduplicate.
- Actual localHTTPservicepipeline forBOTHsourcesnonempty, complete/primaryfail/additionalfail/bothfail/emptyadditional; actualsharedCLIserializer JSON/status/exit0/3/1, partialsuccessfuldataretained/nullunknownfields, classsessionrequestedcontext.
- CapturedGETpaths primaryqueryEXACTisWeb=true/all5roleheaders/Bearer/noCookie/body/secretquery andNOembeddedURLcalls/downloads; redirectsNOTfollowed/noAuthorizationforwarding.
- 401403authvs500upstream,invalidJSON/schema/network/wholedeadlinecombinedheaderbody/trickle,sharedoverallbudgetacross2calls,chunkedno-lengthoverflow. Testonlyloopbackcfgguard exactproductionfixedorigin; testsactualpipeline notduplicatefake. MissingID/unsafeIDusage2beforecredentials/missingcredenvelope actualUTC, bothbinaryentrypointsunchanged.

Prefernew smallsession_details.rs plusminimalCLIwiring/sharedprivatehelpers, notcopywholeapp ormassrefactor. Keepalreadyacceptedtests/prodbehavior. Ifschemafield ambiguity realblocker,saywhichwithoutguessing. No moreprobeneededforbasicfields above. `totalResource` mismatchneedsdocumentedhonesthandlingnotfakedfalsecomplete.

Run cargo test --all-targets,fmt --check,clippyalltargets-Dwarnings,debug/releasebuild,bothhelpandshowhelp. NOliveinsideCodexsandbox; finalpendingparentverify expected. Detailed `.agent/session-show-evidence.md`, separate-o summary `.agent/session-show-summary.md`. Actualcounts/tests/artifactpaths/exactcommands, nofabricatedrealdata. StopafterboundedSHOWscope. Parentwillreviewsourceandrunlive2samplesandrepeatshowtosanity-checkrawprogressfieldsunchangedforobservedsamples; thatdoesnotproveuniversalserverreadsemantics. Remain gpt-6-luna oncapacity; preserveincrementalwork.
